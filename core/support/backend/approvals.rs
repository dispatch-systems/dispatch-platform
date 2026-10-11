use super::{api::types::SupportDetail, authorize, jobs, notifications, proposals, tickets};
use crate::{
    Result,
    accounts::Auth,
    db::{Store, now, s},
    ensure,
};
use rusqlite::params;

pub fn decide(
    db: &Store,
    auth: &Auth,
    ticket: &str,
    version: u32,
    decision: &str,
    note: &str,
) -> Result<SupportDetail> {
    ensure(auth.user.platform_owner, "platform_owner_required", 403)?;
    db.platform.transaction(|| {
        let row = tickets::record(db, ticket)?;
        ensure(row["proposal_version"] == version && version > 0, "support_proposal_changed", 409)?;
        if let Some((saved,)) = db.platform.one_as::<(String,)>(
            "SELECT decision FROM support_approvals WHERE ticket_id=? AND version=?", params![ticket, version],
        )? {
            ensure(saved == decision, "support_decision_exists", 409)?;
            return tickets::detail(db, auth, ticket);
        }
        ensure(s(&row, "status") == "awaiting_approval", "support_not_awaiting_approval", 409)?;
        ensure(["approve", "deny"].contains(&decision), "invalid_input", 400)?;
        proposals::get(db, ticket, version)?;
        db.platform.exec(
            "INSERT INTO support_approvals(ticket_id,version,owner_id,decision,note,created_at) VALUES(?,?,?,?,?,?)",
            params![ticket, version, auth.user.id, decision, note, now()],
        )?;
        if decision == "approve" {
            db.platform.exec("UPDATE support_tickets SET approved_version=? WHERE id=?", params![version, ticket])?;
            tickets::progress(db, ticket, "implementing", "Approved · implementation queued")?;
            jobs::enqueue(db, ticket, "implement", version, None)?;
            notifications::reporter(db, ticket, "Your ticket was approved and is being implemented")?;
        } else {
            jobs::cancel(db, ticket)?;
            tickets::progress(db, ticket, "denied", "The platform owner declined this proposal")?;
            super::conversations::add(db, ticket, &auth.user.name(), "owner", note, false)?;
            notifications::reporter(db, ticket, "The platform owner declined your ticket")?;
        }
        tickets::detail(db, auth, ticket)
    })
}
pub fn resume(db: &Store, auth: &Auth, ticket: &str) -> Result<SupportDetail> {
    ensure(auth.user.platform_owner, "platform_owner_required", 403)?;
    authorize(db, auth, ticket)?;
    db.platform.transaction(|| {
        let changed = db.platform.exec(
            "UPDATE support_jobs SET status='queued',available_at=?,updated_at=? WHERE ticket_id=? AND status='waiting'",
            params![now(), now(), ticket],
        )?;
        ensure(changed > 0, "support_not_waiting", 409)?;
        let row = tickets::record(db, ticket)?;
        let approved = row["approved_version"].as_u64().is_some();
        tickets::progress(db, ticket, if approved { "implementing" } else { "investigating" }, "Work resuming")?;
        tickets::detail(db, auth, ticket)
    })
}
