use super::{conversations, proposals, tickets, verification};
use crate::{
    Result,
    db::{Store, now, s},
    ensure,
    foundation::validate as v,
};
use rusqlite::params;
use serde_json::Value;

pub fn record(db: &Store, body: &Value) -> Result<()> {
    v::fields(
        body,
        &[
            "deliveryId",
            "ticketId",
            "proposalVersion",
            "branch",
            "prUrl",
            "headSha",
            "event",
            "mergeSha",
            "summary",
        ],
    )?;
    let delivery = v::text(body, "deliveryId", 1, 180)?;
    let id = v::text(body, "ticketId", 1, 100)?;
    let version = v::integer(body, "proposalVersion", 1, u32::MAX.into())? as u32;
    let branch = v::text(body, "branch", 1, 100)?;
    let pr = v::text(body, "prUrl", 1, 256)?;
    let head = v::text(body, "headSha", 40, 40)?;
    let event = v::choice(
        body,
        "event",
        &["checks_failed", "conflict", "merged", "review"],
    )?;
    let summary = v::text(body, "summary", 0, 4000)?;
    ensure(
        verification::github_url(pr) && proposals::sha(head),
        "invalid_input",
        400,
    )?;
    let env = if db.config.environment == "production" {
        "production"
    } else {
        "dev"
    };
    ensure(
        branch
            == format!(
                "support-{env}-{}",
                id.strip_prefix("support_").unwrap_or("")
            ),
        "support_branch_mismatch",
        403,
    )?;
    db.platform.transaction(|| {
        let row = tickets::record(db, id)?;
        ensure(row["approved_version"] == version && row["pr_url"] == pr, "support_github_unregistered", 403)?;
        if row["head_sha"] != head { return Ok(()); }
        let inserted = db.platform.exec(
            "INSERT OR IGNORE INTO support_github_events(delivery_id,ticket_id,event,body,created_at) VALUES(?,?,?,?,?)",
            params![delivery, id, event, serde_json::to_string(body)?, now()],
        )?;
        if inserted == 0 { return Ok(()); }
        if event == "merged" {
            ensure(body["mergeSha"].as_str().is_some_and(proposals::sha), "support_merge_unconfirmed", 400)?;
        }
        conversations::add(db, id, "GitHub", "system", summary, true)?;
        if !["merged", "resolved", "denied"].contains(&s(&row, "status")) {
            let progress = match event {
                "checks_failed" => "A required check failed · agent is correcting it",
                "conflict" => "Branch conflict · agent is updating the implementation",
                "review" => "Review feedback received · agent is reviewing it",
                _ => "GitHub confirmed merge · finalizing the ticket",
            };
            tickets::progress(db, id, s(&row, "status"), progress)?;
        }
        Ok(())
    })
}
