use super::{api::types::SupportNotification, tickets};
use crate::{
    Result,
    accounts::Auth,
    db::{FromRow, Row, Store, now, s},
    foundation::crypto,
};
use rusqlite::params;

impl FromRow for SupportNotification {
    fn from_row(r: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: r.get("id")?,
            ticket_id: r.get("ticket_id")?,
            body: r.get("body")?,
            read_at: r.get("read_at")?,
            created_at: r.get("created_at")?,
        })
    }
}
fn send(db: &Store, ticket: &str, user: &str, scope: &str, body: &str) -> Result<()> {
    db.platform.exec(
        "INSERT INTO support_notifications(id,ticket_id,user_id,user_scope,body,created_at) VALUES(?,?,?,?,?,?)",
        params![crypto::id("support_notice")?, ticket, user, scope, body, now()],
    )?;
    Ok(())
}
pub(crate) fn reporter(db: &Store, ticket: &str, body: &str) -> Result<()> {
    let row = tickets::record(db, ticket)?;
    send(
        db,
        ticket,
        s(&row, "reporter_id"),
        s(&row, "reporter_scope"),
        body,
    )
}
pub(crate) fn owners(db: &Store, ticket: &str, body: &str) -> Result<()> {
    // Account ownership stays with accounts; this reads its public authority, not its tables.
    for user in db.active_platform_owners()? {
        send(db, ticket, &user.id, "", body)?;
    }
    Ok(())
}
pub fn list(db: &Store, auth: &Auth) -> Result<Vec<SupportNotification>> {
    db.platform.query_as(
        "SELECT * FROM support_notifications WHERE user_id=? AND user_scope=? \
         ORDER BY (read_at IS NULL) DESC,sequence DESC LIMIT 100",
        params![auth.user.id, auth.scope.as_deref().unwrap_or("")],
    )
}
pub fn read(db: &Store, auth: &Auth, ids: &[String]) -> Result<()> {
    db.platform.transaction(|| {
        for id in ids {
            db.platform.exec(
                "UPDATE support_notifications SET read_at=COALESCE(read_at,?) WHERE id=? AND user_id=? AND user_scope=?",
                params![now(), id, auth.user.id, auth.scope.as_deref().unwrap_or("")],
            )?;
        }
        Ok(())
    })
}
pub fn unread(db: &Store, auth: &Auth) -> Result<i64> {
    db.platform.count("SELECT count(*) FROM support_notifications WHERE user_id=? AND user_scope=? AND read_at IS NULL",
        params![auth.user.id, auth.scope.as_deref().unwrap_or("")])
}
