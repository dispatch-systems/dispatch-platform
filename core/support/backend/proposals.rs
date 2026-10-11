use super::{api::types::SupportProposal, decode, jobs, notifications, tickets};
use crate::{
    Result,
    db::{Store, now},
    ensure,
};
use rusqlite::params;

pub(crate) fn list(db: &Store, ticket: &str) -> Result<Vec<SupportProposal>> {
    db.platform
        .query_as::<(String,)>(
            "SELECT body FROM support_proposals WHERE ticket_id=? ORDER BY version",
            [ticket],
        )?
        .into_iter()
        .map(|(body,)| decode(&body))
        .collect()
}
pub(crate) fn get(db: &Store, ticket: &str, version: u32) -> Result<SupportProposal> {
    let (body,) = super::found(db.platform.one_as::<(String,)>(
        "SELECT body FROM support_proposals WHERE ticket_id=? AND version=?",
        params![ticket, version],
    )?)?;
    decode(&body)
}
pub fn sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn publish(
    db: &Store,
    job: &str,
    lease: &str,
    mut proposal: SupportProposal,
) -> Result<SupportProposal> {
    ensure(sha(&proposal.source_sha), "support_source_required", 400)?;
    ensure(
        !proposal.analysis.trim().is_empty()
            && !proposal.plan.is_empty()
            && !proposal.acceptance_criteria.is_empty()
            && !proposal.verification.is_empty()
            && !proposal.production_verification.is_empty(),
        "support_proposal_incomplete",
        400,
    )?;
    ensure(
        proposal
            .acceptance_criteria
            .iter()
            .chain(proposal.production_verification.iter())
            .all(|c| !c.trim().is_empty()),
        "support_proposal_incomplete",
        400,
    )?;
    db.platform.transaction(|| {
        let run = jobs::leased(db, job, lease)?;
        ensure(
            ["investigate", "answer", "implement"].contains(&run.kind.as_str()),
            "support_job_not_authorized",
            403,
        )?;
        let ticket = tickets::record(db, &run.ticket)?;
        // A published PR keeps its approval immutable while GitHub can merge it.
        ensure(
            ticket["pr_url"].is_null()
                && !matches!(
                    ticket["status"].as_str(),
                    Some("queued" | "merged" | "resolved")
                ),
            "support_proposal_already_published",
            409,
        )?;
        let pinned = ticket["source_sha"].as_str().unwrap_or("");
        ensure(
            pinned.is_empty() || proposal.source_sha == pinned,
            "support_source_changed",
            409,
        )?;
        proposal.version = ticket["proposal_version"].as_u64().unwrap_or(0) as u32 + 1;
        proposal.created_at = now();
        db.platform.exec(
            "INSERT INTO support_proposals(ticket_id,version,body,created_at) VALUES(?,?,?,?)",
            params![
                run.ticket,
                proposal.version,
                serde_json::to_string(&proposal)?,
                proposal.created_at
            ],
        )?;
        db.platform.exec(
            "UPDATE support_tickets SET proposal_version=?,approved_version=NULL WHERE id=?",
            params![proposal.version, run.ticket],
        )?;
        // A materially revised proposal revokes every old implementation authority immediately.
        db.platform.exec(
            "UPDATE support_jobs SET status='cancelled',lease_hash=NULL,lease_until=NULL \
             WHERE ticket_id=? AND kind='implement' AND status IN ('queued','running','waiting')",
            [&run.ticket],
        )?;
        tickets::progress(
            db,
            &run.ticket,
            "awaiting_approval",
            "Investigation ready · waiting for the platform owner",
        )?;
        notifications::owners(db, &run.ticket, "AI investigation and proposal are ready")?;
        notifications::reporter(
            db,
            &run.ticket,
            "Your ticket has been investigated and is awaiting a decision",
        )?;
        Ok(proposal)
    })
}
