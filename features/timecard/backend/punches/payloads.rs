//! Private, versioned stored formats, pooled beside the whole inline JSON every writer stores.
use dispatch_core::{
    Error, Result,
    db::{Db, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;

const PUNCHES: &str = "paycom.punches.v1";
const CAPTURE: &str = "paycom.employee-capture.v1";

/// What a reader selects beside a row's inline JSON for `punches` or `capture` to read its
/// shared payload, from the join below that fits the row.
pub(super) const COLUMNS: &str = "r.format payload_format,b.body payload_body";
/// Joins `timecards t` to its shared punches, when it has them.
pub(super) fn punches_join() -> String {
    format!(
        "LEFT JOIN paycom_timecard_payload_refs r ON r.publication_id=t.publication_id \
         AND r.employee_code=t.employee_code AND r.date=t.date AND r.format='{PUNCHES}' \
         LEFT JOIN paycom_payloads b ON b.format=r.format AND b.digest=r.digest"
    )
}
/// Joins `employee_timecard_syncs s` to its shared capture, when it has one.
pub(super) fn capture_join() -> String {
    format!(
        "LEFT JOIN paycom_employee_sync_payload_refs r ON r.employee_code=s.employee_code \
         AND r.period_from=s.period_from AND r.period_to=s.period_to AND r.format='{CAPTURE}' \
         LEFT JOIN paycom_payloads b ON b.format=r.format AND b.digest=r.digest"
    )
}

fn unavailable() -> Error {
    Error::new("paycom_payload_unavailable", 503)
}

/// V1 sorts object keys explicitly: this crate enables serde_json's preserve_order.
/// Arrays, number variants, string contents and absent/null distinctions are untouched.
fn canonical(value: &Value, out: &mut String) -> Result<()> {
    match value {
        Value::Object(fields) => {
            let mut fields: Vec<_> = fields.iter().collect();
            fields.sort_by_key(|(key, _)| *key);
            out.push('{');
            for (index, (key, value)) in fields.into_iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key)?);
                out.push(':');
                canonical(value, out)?;
            }
            out.push('}');
        }
        Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                canonical(value, out)?;
            }
            out.push(']');
        }
        value => out.push_str(&serde_json::to_string(value)?),
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
pub(super) struct Payload {
    format: &'static str,
    digest: String,
    body: String,
}
impl Payload {
    fn new(format: &'static str, value: &Value) -> Result<Self> {
        let mut body = String::new();
        canonical(value, &mut body)?;
        let mut key = Vec::with_capacity(format.len() + 1 + body.len());
        key.extend_from_slice(format.as_bytes());
        key.push(0);
        key.extend_from_slice(body.as_bytes());
        Ok(Self {
            format,
            digest: crypto::sha(key),
            body,
        })
    }
    pub(super) fn punches(value: &Value) -> Result<Self> {
        ensure(value.is_array(), "paycom_payload_unavailable", 503)?;
        Self::new(PUNCHES, value)
    }
    pub(super) fn capture(value: &Value) -> Result<Self> {
        let mut body = value.clone();
        let fields = body.as_object_mut().ok_or_else(unavailable)?;
        ensure(
            fields
                .remove("collectedAt")
                .is_some_and(|at| at.is_string()),
            "paycom_payload_unavailable",
            503,
        )?;
        Self::new(CAPTURE, &body)
    }
    pub(super) fn digest(&self) -> &str {
        &self.digest
    }
    /// Called only inside the transaction that writes the rows it's for, once per distinct
    /// payload.
    pub(super) fn intern(&self, db: &Db) -> Result<()> {
        db.exec(
            "INSERT INTO paycom_payloads(format,digest,body) VALUES (?,?,?) \
             ON CONFLICT(format,digest) DO NOTHING",
            params![self.format, self.digest, self.body],
        )?;
        let existing: Option<(String,)> = db.one_as(
            "SELECT body FROM paycom_payloads WHERE format=? AND digest=?",
            params![self.format, self.digest],
        )?;
        ensure(
            existing.is_some_and(|(body,)| body == self.body),
            "paycom_payload_collision",
            503,
        )
    }
    pub(super) fn attach_card(
        &self,
        db: &Db,
        publication: &str,
        code: &str,
        date: &str,
    ) -> Result<()> {
        db.exec(
            "INSERT INTO paycom_timecard_payload_refs \
             (publication_id,employee_code,date,format,digest) VALUES (?,?,?,?,?)",
            params![publication, code, date, self.format, self.digest],
        )?;
        Ok(())
    }
    pub(super) fn attach_sync(
        &self,
        db: &Db,
        code: &str,
        from: &str,
        to: &str,
        collected_at: &str,
    ) -> Result<()> {
        db.exec(
            "INSERT INTO paycom_employee_sync_payload_refs \
             (employee_code,period_from,period_to,format,digest,payload_collected_at) \
             VALUES (?,?,?,?,?,?)",
            params![code, from, to, self.format, self.digest, collected_at],
        )?;
        Ok(())
    }
}

/// A supported joined body wins. An absent/unknown ref may use only complete inline JSON.
/// Exact compact markers deliberately fail this shape check when their payload is missing.
fn decode(inline: &str, format: Option<&str>, body: Option<&str>, expected: &str) -> Result<Value> {
    let text = if format == Some(expected) {
        body.unwrap_or(inline)
    } else {
        inline
    };
    let value: Value = serde_json::from_str(text).map_err(|_| unavailable())?;
    ensure(
        if expected == PUNCHES {
            value.is_array()
        } else {
            value.is_object() && value.get("$dispatchPayload").is_none()
        },
        "paycom_payload_unavailable",
        503,
    )?;
    Ok(value)
}

/// Query-local aliases are removed before returning any public row.
pub(super) fn punches(row: &mut Value) -> Result<()> {
    let value = decode(
        s(row, "punches"),
        row["payload_format"].as_str(),
        row["payload_body"].as_str(),
        PUNCHES,
    )?;
    // Validate the shared stored shape for the untyped operator probe as well.
    Vec::<crate::Punch>::deserialize(&value).map_err(|_| unavailable())?;
    let fields = row.as_object_mut().ok_or_else(unavailable)?;
    fields.remove("payload_format");
    fields.remove("payload_body");
    fields.insert("punches".into(), value);
    Ok(())
}

pub(super) fn capture(row: &Value) -> Result<Value> {
    let shared = row["payload_format"] == CAPTURE && row["payload_body"].is_string();
    let mut value = decode(
        s(row, "data"),
        row["payload_format"].as_str(),
        row["payload_body"].as_str(),
        CAPTURE,
    )?;
    if shared {
        // Never substitute the ordering column: old data can have a different raw timestamp.
        ensure(
            value.get("collectedAt").is_none(),
            "paycom_payload_unavailable",
            503,
        )?;
        value["collectedAt"] = row["payload_collected_at"]
            .as_str()
            .ok_or_else(unavailable)?
            .into();
    }
    // Validated writes established the format. Check its envelope/shape without rehashing.
    ensure(
        value["collectedAt"].is_string()
            && value["employees"]
                .as_array()
                .is_some_and(|employees| employees.len() == 1)
            && value["timecards"].is_array(),
        "paycom_payload_unavailable",
        503,
    )?;
    dispatch_paycom::timecards::validate_workforce(&value).map_err(|_| unavailable())?;
    Ok(value)
}

#[cfg(test)]
#[path = "../../tests/backend/punches/payloads.rs"]
mod tests;
