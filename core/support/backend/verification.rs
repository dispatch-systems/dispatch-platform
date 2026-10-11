use super::{api::types::SupportEvidence, decode, jobs, notifications, proposals, tickets};
use crate::{
    Result,
    db::{Store, now, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;
use serde_json::Value;

pub(crate) fn list(db: &Store, ticket: &str) -> Result<Vec<SupportEvidence>> {
    db.platform
        .query_as::<(String,)>(
            "SELECT body FROM support_evidence WHERE ticket_id=? ORDER BY sequence",
            [ticket],
        )?
        .into_iter()
        .map(|(body,)| decode(&body))
        .collect()
}
pub fn record(
    db: &Store,
    job: &str,
    lease: &str,
    mut evidence: SupportEvidence,
) -> Result<SupportEvidence> {
    db.platform.transaction(|| {
        let run = jobs::leased(db, job, lease)?;
        ensure(
            ["investigation", "verification", "collection", "release"]
                .contains(&evidence.phase.as_str()),
            "invalid_input",
            400,
        )?;
        ensure(
            proposals::sha(&evidence.source_sha)
                && !evidence.steps.is_empty()
                && !evidence.outcome.trim().is_empty()
                && !evidence.criterion.trim().is_empty(),
            "support_evidence_incomplete",
            400,
        )?;
        for id in &evidence.attachment_ids {
            ensure(db.platform.count(
                "SELECT count(*) FROM support_attachments WHERE id=? AND ticket_id=? AND private=1",
                params![id, run.ticket],
            )? == 1, "support_attachment_unavailable", 409)?;
        }
        evidence.id = crypto::id("support_evidence")?;
        evidence.created_at = now();
        db.platform.exec(
            "INSERT INTO support_evidence(id,job_id,ticket_id,body,created_at) VALUES(?,?,?,?,?)",
            params![
                evidence.id,
                job,
                run.ticket,
                serde_json::to_string(&evidence)?,
                evidence.created_at
            ],
        )?;
        Ok(evidence)
    })
}
pub(crate) fn verified(
    db: &Store,
    ticket: &str,
    version: u32,
    head: &str,
    phase: &str,
) -> Result<()> {
    ensure(proposals::sha(head), "support_source_required", 400)?;
    let proposal = proposals::get(db, ticket, version)?;
    let evidence = list(db, ticket)?;
    let criteria = if phase == "release" {
        proposal.production_verification
    } else {
        proposal.acceptance_criteria
    };
    for criterion in criteria {
        // A later failing observation invalidates an earlier pass of the same criterion/head.
        let latest = evidence.iter().rev().find(|item| {
            item.phase == phase && item.source_sha == head && item.criterion == criterion
        });
        ensure(
            latest.is_some_and(|item| item.passed),
            "support_verification_required",
            409,
        )?;
    }
    Ok(())
}
pub fn github_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url
                .path()
                .strip_prefix("/dispatch-systems/dispatch-platform/pull/")
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
    })
}
pub fn github(db: &Store, job: &str, lease: &str, pr: &str, head: &str) -> Result<()> {
    db.platform.transaction(|| {
        let run = jobs::leased(db, job, lease)?;
        ensure(run.kind == "implement", "support_job_not_authorized", 403)?;
        ensure(github_url(pr), "invalid_input", 400)?;
        verified(db, &run.ticket, run.version, head, "verification")?;
        db.platform.exec(
            "UPDATE support_tickets SET pr_url=?,head_sha=?,updated_at=? WHERE id=?",
            params![pr, head, now(), run.ticket],
        )?;
        Ok(())
    })
}
pub fn complete(db: &Store, job: &str, lease: &str, result: &Value) -> Result<()> {
    db.platform.transaction(|| {
        let run = jobs::leased(db, job, lease)?;
        let ticket = tickets::record(db, &run.ticket)?;
        match run.kind.as_str() {
            "investigate" => {
                ensure(
                    s(&ticket, "status") == "awaiting_approval",
                    "support_proposal_required",
                    409,
                )?;
            }
            "answer" => {
                let answer = result["answer"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| crate::Error::new("invalid_input", 400))?;
                super::conversations::add(db, &run.ticket, "Hermes", "agent", answer, true)?;
                notifications::owners(db, &run.ticket, "Hermes answered your question")?;
            }
            "implement" => {
                let pr = result["prUrl"].as_str().unwrap_or("");
                let head = result["headSha"].as_str().unwrap_or("");
                let merged = result["mergeSha"].as_str().unwrap_or("");
                ensure(
                    proposals::sha(merged) && ticket["pr_url"] == pr && ticket["head_sha"] == head,
                    "support_merge_unconfirmed",
                    409,
                )?;
                verified(db, &run.ticket, run.version, head, "verification")?;
                db.platform.exec(
                    "UPDATE support_tickets SET merge_sha=? WHERE id=?",
                    params![merged, run.ticket],
                )?;
                tickets::progress(
                    db,
                    &run.ticket,
                    "merged",
                    "Merged · awaiting production release",
                )?;
                notifications::reporter(
                    db,
                    &run.ticket,
                    "Your change was merged and is awaiting a production release",
                )?;
                notifications::owners(db, &run.ticket, "AI Support implementation merged")?;
                jobs::enqueue(db, &run.ticket, "release", run.version, None)?;
            }
            "release" => {
                let release = result["release"].as_str().unwrap_or("");
                let head = result["headSha"].as_str().unwrap_or("");
                let merged = result["mergeSha"].as_str().unwrap_or("");
                ensure(
                    !release.is_empty() && ticket["merge_sha"] == merged,
                    "support_release_unconfirmed",
                    409,
                )?;
                verified(db, &run.ticket, run.version, head, "release")?;
                db.platform.exec(
                    "UPDATE support_tickets SET released_version=? WHERE id=?",
                    params![release, run.ticket],
                )?;
                tickets::progress(
                    db,
                    &run.ticket,
                    "resolved",
                    "Released and verified in production",
                )?;
                notifications::reporter(db, &run.ticket, "Your change is available in production")?;
            }
            _ => return Err(crate::Error::new("support_job_not_authorized", 403)),
        }
        jobs::finish(db, job)
    })
}
