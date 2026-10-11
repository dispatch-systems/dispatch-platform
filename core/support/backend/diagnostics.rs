//! Bounded diagnostics derived from an authorized ticket, with no credentials or employee data.
use super::tickets;
use crate::{
    Result,
    db::{Store, now, s},
    ensure,
};
use serde_json::{Value, json};

pub fn diagnostic_dsp(db: &Store, ticket: &str, dsp: &str, actor: &str) -> Result<Value> {
    let row = tickets::record(db, ticket)?;
    ensure(
        row["dsp_id"] == dsp
            && (row["reporter_id"] == actor || db.active_platform_owner(actor)?.is_some()),
        "support_ticket_not_found",
        404,
    )?;
    worker(db, ticket)
}
pub(crate) fn worker(db: &Store, ticket: &str) -> Result<Value> {
    let row = tickets::record(db, ticket)?;
    let collected = match row["dsp_id"].as_str() {
        Some(dsp) => crate::collection::verification::diagnostics(
            db,
            dsp,
            row["created_at"].as_i64().unwrap_or(now()) - 86400000,
        )?,
        None => Value::Null,
    };
    Ok(json!({
        "ticketId":ticket,"reporterName":s(&row,"reporter_name"),
        "dspId":row["dsp_id"],"dspName":row["dsp_name"],
        "reportedRelease":s(&row,"release"),"reportedSourceSha":s(&row,"source_sha"),
        "runtime":{"release":db.config.release,"sourceSha":db.config.source.commit,"version":db.config.source.version},
        "recentCollections":collected,"sampleLimit":20,"checkedAt":now()
    }))
}
