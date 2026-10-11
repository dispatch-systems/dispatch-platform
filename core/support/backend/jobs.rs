use super::{api::types::SupportJob, decode, notifications, tickets};
use crate::{
    Result,
    db::{Store, now, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::{Value, json};

const LEASE_MS: i64 = 90_000;
pub(crate) struct Lease {
    pub ticket: String,
    pub kind: String,
    pub version: u32,
}
pub(crate) fn enqueue(
    db: &Store,
    ticket: &str,
    kind: &str,
    version: u32,
    question: Option<&str>,
) -> Result<()> {
    let time = now();
    db.platform.exec(
        "INSERT OR IGNORE INTO support_jobs(id,ticket_id,kind,version,question_id,available_at,created_at,updated_at) \
         VALUES(?,?,?,?,?,?,?,?)",
        params![crypto::id("support_job")?, ticket, kind, version, question, time, time, time],
    )?;
    Ok(())
}
pub(crate) fn cancel(db: &Store, ticket: &str) -> Result<()> {
    db.platform.exec(
        "UPDATE support_jobs SET status='cancelled',lease_hash=NULL,lease_until=NULL,updated_at=? \
         WHERE ticket_id=? AND status IN ('queued','running','waiting')",
        params![now(), ticket],
    )?;
    Ok(())
}

/// A clarification first returns to tool-free routing, never directly to heavy work.
pub(crate) fn route_reply(
    db: &Store,
    ticket: &str,
    version: u32,
    previous_status: &str,
    reply: &str,
) -> Result<()> {
    let previous_status = if previous_status == "triaging" {
        let current = db.platform.one_as::<(String,)>(
            "SELECT checkpoint FROM support_jobs WHERE ticket_id=? AND kind='investigate' \
             AND version=? AND status IN ('queued','running','waiting') ORDER BY created_at DESC,rowid DESC LIMIT 1",
            params![ticket, version],
        )?;
        let current: Value = current
            .map(|(text,)| decode(&text))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        if let Some(previous) = current["previousStatus"].as_str() {
            previous.to_owned()
        } else {
            super::intake::latest(db, ticket, version)?.map_or_else(
                || previous_status.to_owned(),
                |saved| saved.outcome.status().to_owned(),
            )
        }
    } else {
        previous_status.to_owned()
    };
    db.platform.exec(
        "UPDATE support_jobs SET status='cancelled',lease_hash=NULL,lease_until=NULL,updated_at=? \
         WHERE ticket_id=? AND kind='investigate' AND status IN ('queued','running','waiting')",
        params![now(), ticket],
    )?;
    enqueue(db, ticket, "investigate", version, None)?;
    let checkpoint = json!({"phase":"routing","previousStatus":previous_status,"replyId":reply});
    db.platform.exec(
        "UPDATE support_jobs SET checkpoint=? WHERE ticket_id=? AND kind='investigate' AND version=? AND status='queued'",
        params![serde_json::to_string(&checkpoint)?, ticket, version],
    )?;
    tickets::progress(db, ticket, "triaging", "Routing your reply")
}
pub(crate) fn leased(db: &Store, job: &str, lease: &str) -> Result<Lease> {
    let row = db.platform.one(
        "SELECT * FROM support_jobs WHERE id=? AND status='running' AND lease_hash=? AND lease_until>?",
        params![job, crypto::sha(lease), now()],
    )?.ok_or_else(|| crate::Error::new("support_lease_lost", 409))?;
    let run = Lease {
        ticket: s(&row, "ticket_id").to_owned(),
        kind: s(&row, "kind").to_owned(),
        version: row["version"].as_u64().unwrap_or(0) as u32,
    };
    if run.kind == "implement" || run.kind == "release" {
        let ticket = tickets::record(db, &run.ticket)?;
        ensure(
            ticket["approved_version"] == run.version,
            "support_approval_revoked",
            409,
        )?;
        let owner = db.platform.one_as::<(String,)>(
            "SELECT owner_id FROM support_approvals WHERE ticket_id=? AND version=? AND decision='approve'",
            params![run.ticket, run.version],
        )?;
        ensure(
            owner.is_some_and(|(id,)| db.active_platform_owner(&id).ok().flatten().is_some()),
            "support_approval_revoked",
            409,
        )?;
    }
    Ok(run)
}
pub fn claim(db: &Store) -> Result<Option<SupportJob>> {
    db.platform.transaction(|| {
        let time = now();
        let next = db.platform.one(
            "SELECT j.* FROM support_jobs j JOIN support_tickets t ON t.id=j.ticket_id \
             WHERE ((j.status='queued' AND j.available_at<=?) OR (j.status='running' AND j.lease_until<=?)) \
             AND t.status NOT IN ('denied','resolved') \
             AND (j.kind NOT IN ('implement','release') OR j.version=t.approved_version) \
             ORDER BY j.available_at,j.created_at,j.id LIMIT 1", params![time, time],
        )?;
        let Some(row) = next else { return Ok(None); };
        let id = s(&row, "id");
        let ticket = s(&row, "ticket_id");
        let raw = crypto::token()?;
        let expires = time + LEASE_MS;
        db.platform.exec(
            "UPDATE support_jobs SET status='running',lease_hash=?,lease_until=?,attempts=attempts+1,updated_at=? WHERE id=?",
            params![crypto::sha(&raw), expires, time, id],
        )?;
        Ok(Some(SupportJob {
            id: id.into(), kind: s(&row, "kind").into(), version: row["version"].as_u64().unwrap_or(0) as u32,
            lease: raw, lease_until: expires, checkpoint: decode(s(&row, "checkpoint"))?,
            question_id: row["question_id"].as_str().map(str::to_owned),
            environment: if db.config.environment == "production" { "production" } else { "dev" }.into(),
            origin: db.config.origin.clone(), detail: tickets::view(db, ticket, true)?,
        }))
    })
}
pub fn heartbeat(db: &Store, job: &str, lease: &str, checkpoint: &Value) -> Result<i64> {
    db.platform.transaction(|| {
        leased(db, job, lease)?;
        ensure(checkpoint.is_object(), "invalid_input", 400)?;
        let expires = now() + LEASE_MS;
        db.platform.exec(
            "UPDATE support_jobs SET checkpoint=?,lease_until=?,updated_at=? WHERE id=?",
            params![serde_json::to_string(checkpoint)?, expires, now(), job],
        )?;
        Ok(expires)
    })
}
pub fn progress(
    db: &Store,
    job: &str,
    lease: &str,
    status: &str,
    progress: &str,
    checkpoint: &Value,
) -> Result<()> {
    db.platform.transaction(|| {
        let run = leased(db, job, lease)?;
        let allowed = match run.kind.as_str() {
            "implement" => ["implementing", "verifying", "queued"].contains(&status),
            "investigate" => ["triaging", "investigating", "needs_information"].contains(&status),
            "release" => status == "merged",
            _ => false,
        };
        ensure(allowed, "support_transition_refused", 409)?;
        heartbeat(db, job, lease, checkpoint)?;
        tickets::progress(db, &run.ticket, status, progress)
    })
}
pub fn wait(
    db: &Store,
    job: &str,
    lease: &str,
    reason: &str,
    checkpoint: &Value,
    retry_ms: Option<i64>,
) -> Result<()> {
    db.platform.transaction(|| {
        let run = leased(db, job, lease)?;
        ensure(checkpoint.is_object(), "invalid_input", 400)?;
        let (status, available) = retry_ms.map_or(("waiting", now()), |delay| ("queued", now() + delay));
        db.platform.exec(
            "UPDATE support_jobs SET status=?,available_at=?,checkpoint=?,lease_hash=NULL,lease_until=NULL,updated_at=? WHERE id=?",
            params![status, available, serde_json::to_string(checkpoint)?, now(), job],
        )?;
        // Waiting for a published release is a successful merge, not an implementation failure.
        if run.kind != "release" {
            let visible_status = if retry_ms.is_some() && checkpoint["phase"] == "queued" {
                "queued"
            } else {
                "waiting"
            };
            tickets::progress(db, &run.ticket, visible_status, reason)?;
            if retry_ms.is_none() {
                notifications::owners(db, &run.ticket, "AI Support needs attention to continue")?;
            }
        }
        Ok(())
    })
}
pub(crate) fn finish(db: &Store, job: &str) -> Result<()> {
    db.platform.exec(
        "UPDATE support_jobs SET status='done',lease_hash=NULL,lease_until=NULL,updated_at=? WHERE id=?", params![now(), job],
    )?;
    Ok(())
}
pub fn checkpoint(db: &Store, job: &str, lease: &str) -> Result<Value> {
    leased(db, job, lease)?;
    let (body,) = db
        .platform
        .one_as::<(String,)>("SELECT checkpoint FROM support_jobs WHERE id=?", [job])?
        .ok_or_else(|| crate::Error::new("support_lease_lost", 409))?;
    decode(&body)
}
pub fn status(db: &Store, job: &str, lease: &str) -> Result<Value> {
    let run = leased(db, job, lease)?;
    Ok(
        json!({"ticket":tickets::view(db, &run.ticket, true)?, "checkpoint":checkpoint(db, job, lease)?}),
    )
}
