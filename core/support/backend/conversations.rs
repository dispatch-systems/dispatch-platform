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
        let row = tickets::record(db, ticket)?;
        let reporter = row["reporter_id"] == auth.user.id
            && row["reporter_scope"] == auth.scope.as_deref().unwrap_or("");
        let role = if auth.user.platform_owner && (private || !reporter) {
            "owner"
        } else {
            "reporter"
        };
        let message = add(db, ticket, &auth.user.name(), role, body.trim(), private)?;
        let version = row["proposal_version"].as_u64().unwrap_or(0) as u32;
        if private {
            ensure(
                !["denied", "resolved"].contains(&s(&row, "status")),
                "support_ticket_closed",
                409,
            )?;
            jobs::enqueue(db, ticket, "answer", version, Some(&message))?;
        } else if reporter {
            notifications::owners(db, ticket, "The reporter replied to an AI Support ticket")?;
            if row["approved_version"].is_null()
                && row["pr_url"].is_null()
                && [
                    "triaging",
                    "needs_information",
                    "acknowledged",
                    "answered",
                    "unclear_bug",
                    "unclear_feature",
                ]
                .contains(&s(&row, "status"))
            {
                jobs::route_reply(db, ticket, version, s(&row, "status"), &message)?;
            }
        } else if auth.user.platform_owner {
            notifications::reporter(db, ticket, "Platform support sent you a message")?;
        }
        Ok(())
    })
}
