use super::{api::types::SupportMessage, authorize, jobs, notifications, tickets};
use crate::{
    Result,
    accounts::Auth,
    db::{FromRow, Row, Store, now, s},
    ensure,
    foundation::crypto,
};
use rusqlite::params;

impl FromRow for SupportMessage {
    fn from_row(r: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            author: r.get("author")?,
            role: r.get("role")?,
            body: r.get("body")?,
            private: r.get("private")?,
            created_at: r.get("created_at")?,
        })
    }
}
pub(crate) fn list(db: &Store, ticket: &str, private: bool) -> Result<Vec<SupportMessage>> {
    db.platform.query_as(
        "SELECT * FROM support_messages WHERE ticket_id=? AND (private=0 OR ?) ORDER BY sequence",
        params![ticket, private],
    )
}
pub(crate) fn add(
    db: &Store,
    ticket: &str,
    author: &str,
    role: &str,
    body: &str,
    private: bool,
) -> Result<String> {
    let id = crypto::id("support_message")?;
    db.platform.exec(
        "INSERT INTO support_messages(id,ticket_id,author,role,body,private,created_at) VALUES(?,?,?,?,?,?,?)",
        params![id, ticket, author, role, body, private, now()],
    )?;
    Ok(id)
}
pub fn reply(db: &Store, auth: &Auth, ticket: &str, body: &str, target: &str) -> Result<()> {
    authorize(db, auth, ticket)?;
    ensure(
        target == "reporter" || (target == "agent" && auth.user.platform_owner),
        "permission_denied",
        403,
    )?;
    ensure(!body.trim().is_empty(), "invalid_input", 400)?;
    db.platform.transaction(|| {
        let private = target == "agent";
        let role = if auth.user.platform_owner {
            "owner"
        } else {
            "reporter"
        };
        let message = add(db, ticket, &auth.user.name(), role, body.trim(), private)?;
        let row = tickets::record(db, ticket)?;
        let version = row["proposal_version"].as_u64().unwrap_or(0) as u32;
        if private {
            ensure(
                !["denied", "resolved"].contains(&s(&row, "status")),
                "support_ticket_closed",
                409,
            )?;
            jobs::enqueue(db, ticket, "answer", version, Some(&message))?;
        } else if auth.user.platform_owner {
            notifications::reporter(db, ticket, "Platform support sent you a message")?;
        } else {
            notifications::owners(db, ticket, "The reporter replied to an AI Support ticket")?;
            if s(&row, "status") == "needs_information" {
                jobs::cancel(db, ticket)?;
                tickets::progress(
                    db,
                    ticket,
                    "investigating",
                    "Reporter replied · investigation resuming",
                )?;
                jobs::enqueue(db, ticket, "investigate", version, None)?;
            }
        }
        Ok(())
    })
}
