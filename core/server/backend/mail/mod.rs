pub use crate::platform_owner::api::types::TransportHealth;
use crate::{
    Result, State,
    accounts::api::types::MailMessage,
    db::{self, Store, n},
    ensure,
    platform_owner::api::types::MailHealth,
};
use rusqlite::params;
use std::collections::HashMap;

mod delivery;
pub mod templates;
pub use delivery::mailer;

/// Never send an expired, revoked or already consumed grant. Bound retained content. A
/// notice about a connected app goes only to someone who is still an active platform owner,
/// and what a feature writes a member only while the member's account is active. What a
/// message is for lives in the directory of the DSP it names, or the platform's.
pub fn discard_stale(db: &Store) -> Result<()> {
    db.platform.exec(
        "DELETE FROM outbox WHERE (status<>'pending' AND created_at<?1) OR \
         (status IN ('pending','failed') AND created_at<?2)",
        params![db::now() - 30 * 86400000, db::now() - 7 * 86400000],
    )?;
    let waiting = db.platform.all(
        "SELECT o.id,o.kind,o.invitation_hash,o.user_id,COALESCE(o.dsp_id,r.dsp_id) dsp_id \
         FROM outbox o LEFT JOIN invitation_routes r ON r.hash=o.invitation_hash \
         WHERE o.status IN ('pending','failed')",
        [],
    )?;
    for row in waiting {
        let text = |key: &str| row[key].as_str();
        if !current(
            db,
            text("kind"),
            text("invitation_hash"),
            text("user_id"),
            text("dsp_id"),
        )? {
            db.platform
                .exec("DELETE FROM outbox WHERE id=?", [text("id").unwrap_or("")])?;
        }
    }
    Ok(())
}

/// Whether what a waiting message is for still stands: an open invitation of an active DSP,
/// an unused reset of an active account, an active platform owner for a notice, and an
/// active account for what a feature writes.
fn current(
    db: &Store,
    kind: Option<&str>,
    invitation: Option<&str>,
    user: Option<&str>,
    dsp: Option<&str>,
) -> Result<bool> {
    let active = |dsp: &str| {
        db.find_dsp(dsp)
            .is_ok_and(|found| found.status == crate::tenancy::api::types::DspStatus::Active)
    };
    // A DSP without its database, or no longer there, holds nothing a message is for.
    let people = |dsp: Option<&str>| db.directory(dsp).ok();
    Ok(match kind {
        Some("invitation") => match (
            invitation,
            dsp.filter(|dsp| active(dsp))
                .and_then(|dsp| people(Some(dsp))),
        ) {
            (Some(hash), Some(people)) => people
                .one(
                    "SELECT 1 FROM invitations WHERE hash=? AND used_at IS NULL AND expires_at>?",
                    params![hash, db::now()],
                )?
                .is_some(),
            _ => false,
        },
        Some("reset") => match (user, people(dsp)) {
            (Some(user), Some(people)) => people
                .one(
                    "SELECT 1 FROM resets r JOIN users u ON u.id=r.user_id WHERE r.user_id=? \
                     AND r.used_at IS NULL AND r.expires_at>? AND u.status='active' \
                     AND r.user_version=u.version",
                    params![user, db::now()],
                )?
                .is_some(),
            _ => false,
        },
        // A notice to a platform owner (`notify_platform_owners`).
        Some(kind) if !kind.contains('.') && dsp.is_none() => match user {
            Some(user) => db
                .platform
                .one(
                    "SELECT 1 FROM users WHERE id=? AND status='active' AND platform_owner=1",
                    [user],
                )?
                .is_some(),
            None => false,
        },
        Some(kind) if kind.contains('.') => match (user, dsp.and_then(|dsp| people(Some(dsp)))) {
            (Some(user), Some(people)) => people
                .one("SELECT 1 FROM users WHERE id=? AND status='active'", [user])?
                .is_some(),
            _ => false,
        },
        _ => true,
    })
}

pub fn transport_status(state: &State, error: Option<&str>) {
    if let Ok(mut health) = state.mail_transport.lock() {
        *health = TransportHealth {
            error: error.map(str::to_owned),
            checked_at: Some(db::iso()),
        };
    }
}

pub fn health(db: &Store, state: &State) -> Result<MailHealth> {
    let counts = db.platform.one("SELECT count(*) FILTER (WHERE status='pending') \
        pending,count(*) FILTER (WHERE status='failed') failed,MIN(created_at) FILTER (WHERE \
        status='pending') oldest,count(*) FILTER (WHERE status='pending' AND created_at IS NULL) unknownAge FROM outbox", [])?.unwrap();
    let last_sent = db.platform.one(
        "SELECT sent_at FROM outbox WHERE sent_at IS NOT NULL ORDER BY sent_at DESC LIMIT 1",
        [],
    )?;
    let last_attempt = db.platform.one(
        "SELECT last_error,last_attempt_at FROM outbox WHERE \
        last_attempt_at IS NOT NULL ORDER BY last_attempt_at DESC,id DESC LIMIT 1",
        [],
    )?;
    let transport = state
        .mail_transport
        .lock()
        .map_err(|_| crate::Error::new("mail_health_unavailable", 503))?
        .clone();
    Ok(MailHealth {
        enabled: db.config.mail_available(),
        pending: n(&counts, "pending"),
        failed: n(&counts, "failed"),
        oldest_pending_age_ms: if n(&counts, "unknownAge") > 0 {
            None
        } else {
            counts["oldest"].as_i64().map(|t| (db::now() - t).max(0))
        },
        last_success_at: last_sent
            .as_ref()
            .and_then(|r| r["sent_at"].as_str())
            .map(str::to_owned),
        last_attempt_at: last_attempt
            .as_ref()
            .and_then(|r| r["last_attempt_at"].as_i64())
            .map(db::at),
        last_error: last_attempt
            .as_ref()
            .and_then(|r| r["last_error"].as_str())
            .map(str::to_owned),
        transport,
    })
}

pub fn record_delivery(db: &Store, id: &str, attempts: i64, error: Option<&str>) -> Result<()> {
    let changed = if let Some(error) = error {
        db.platform.exec(
            "UPDATE outbox SET \
            attempts=attempts+1,status=?,available_at=?,last_attempt_at=?,last_error=? WHERE \
            id=? AND status='pending'",
            params![
                if attempts >= 4 { "failed" } else { "pending" },
                db::now() + 60000 * 2_i64.pow(attempts.clamp(0, 8) as u32),
                db::now(),
                error,
                id
            ],
        )?
    } else {
        db.platform.exec(
            "UPDATE outbox SET \
            status='sent',encrypted_message='',sent_at=?,last_attempt_at=?,last_error=NULL \
            WHERE id=? AND status='pending'",
            params![db::iso(), db::now(), id],
        )?
    };
    ensure(changed == 1, "email_delivery_record_missing", 500)
}

const LOG: &str = "SELECT o.id,o.kind,o.status,o.attempts,o.created_at,o.sent_at,\
    o.last_attempt_at,o.available_at,o.last_error,o.invitation_hash,o.user_id,\
    COALESCE(o.dsp_id,r.dsp_id) dsp_id FROM outbox o \
    LEFT JOIN invitation_routes r ON r.hash=o.invitation_hash \
    ORDER BY COALESCE(o.created_at,0) DESC,o.id DESC LIMIT 200";
// An invitation as its DSP's directory keeps it, with its role and who sent it.
const INVITATION: &str = "SELECT i.email,i.used_at,i.created_by,r.name role,\
    COALESCE(r.system,0) owner FROM invitations i LEFT JOIN roles r ON r.id=i.role_id \
    WHERE i.hash=?";

/// Who a message went to, and for an invitation, what it offered and became, as the
/// directory of the DSP it names keeps them.
struct Sent {
    recipient: Option<String>,
    role: Option<String>,
    owner: bool,
    invited_by: Option<String>,
    used_at: Option<i64>,
}
fn sent(db: &Store, row: &serde_json::Value) -> Result<Sent> {
    let text = |key: &str| row[key].as_str();
    let dsp = text("dsp_id");
    let mut sent = Sent {
        recipient: None,
        role: None,
        owner: false,
        invited_by: None,
        used_at: None,
    };
    if let (Some(hash), Some(dsp)) = (text("invitation_hash"), dsp) {
        let Ok(people) = db.dsp(dsp) else {
            return Ok(sent);
        };
        if let Some(invitation) = people.one(INVITATION, [hash])? {
            sent.recipient = invitation["email"].as_str().map(str::to_owned);
            sent.role = invitation["role"].as_str().map(str::to_owned);
            sent.owner = invitation["owner"].as_i64() == Some(1);
            sent.used_at = invitation["used_at"].as_i64();
            // A platform owner sends as the platform, never by name.
            if let Some(sender) = invitation["created_by"].as_str()
                && !db.platform_owner(sender)?
            {
                sent.invited_by = db.actor_name(dsp, sender)?;
            }
        }
    } else if let Some(user) = text("user_id") {
        let found: Option<(String,)> = match dsp {
            Some(dsp) => match db.dsp(dsp) {
                Ok(people) => people.one_as("SELECT email FROM users WHERE id=?", [user])?,
                Err(_) => None,
            },
            None => db
                .platform
                .one_as("SELECT email FROM users WHERE id=?", [user])?,
        };
        sent.recipient = found.map(|(email,)| email);
    }
    Ok(sent)
}

/// The newest 200 messages, each with what became of the invitation it carried.
pub fn log(db: &Store) -> Result<Vec<MailMessage>> {
    let mut setup: HashMap<String, bool> = HashMap::new();
    db.platform
        .all(LOG, [])?
        .into_iter()
        .map(|row| {
            let text = |key: &str| row[key].as_str().map(str::to_owned);
            let at = |key: &str| row[key].as_i64().map(db::at);
            let sent = sent(db, &row)?;
            let dsp = row["dsp_id"]
                .as_str()
                .filter(|dsp| db.find_dsp(dsp).is_ok());
            let setup_complete = match (sent.owner, dsp) {
                (true, Some(dsp)) => Some(match setup.get(dsp) {
                    Some(done) => *done,
                    None => {
                        let done = !db.profile(dsp)?.setup_required;
                        setup.insert(dsp.to_owned(), done);
                        done
                    }
                }),
                _ => None,
            };
            let pending = row["status"] == "pending";
            Ok(MailMessage {
                id: text("id").unwrap_or_default(),
                kind: text("kind"),
                status: text("status").unwrap_or_default(),
                attempts: row["attempts"].as_i64().unwrap_or(0),
                queued_at: at("created_at"),
                sent_at: text("sent_at"),
                last_attempt_at: at("last_attempt_at"),
                next_attempt_at: if pending { at("available_at") } else { None },
                last_error: text("last_error"),
                recipient: sent.recipient,
                role: sent.role,
                owner: sent.owner,
                dsp_name: match (row["invitation_hash"].is_string(), dsp) {
                    (true, Some(dsp)) => Some(db.find_dsp(dsp)?.name),
                    _ => None,
                },
                invited_by: sent.invited_by,
                accepted_at: sent.used_at.map(db::at),
                setup_complete,
            })
        })
        .collect()
}

/// Changes a message that ran out of attempts, and records who did it and to whose mail.
fn change_failed(
    db: &Store,
    actor: &str,
    id: &str,
    action: &str,
    change: impl FnOnce() -> Result<usize>,
) -> Result<()> {
    db.platform.transaction(|| {
        let row = db.platform.one(
            "SELECT o.invitation_hash,o.user_id,COALESCE(o.dsp_id,r.dsp_id) dsp_id FROM outbox o \
             LEFT JOIN invitation_routes r ON r.hash=o.invitation_hash \
             WHERE o.id=? AND o.status='failed'",
            [id],
        )?;
        let row = row.ok_or_else(|| crate::Error::new("email_not_failed", 409))?;
        let recipient = sent(db, &row)?.recipient;
        change()?;
        db.audit_with(Some(actor), None, action, "", recipient.as_deref(), &[])
    })
}

/// Gives a message that ran out of attempts a fresh set, starting now.
pub fn retry(db: &Store, actor: &str, id: &str) -> Result<()> {
    change_failed(db, actor, id, "mail.retried", || {
        db.platform.exec(
            "UPDATE outbox SET status='pending',attempts=0,available_at=?,last_error=NULL \
             WHERE id=?",
            params![db::now(), id],
        )
    })?;
    db.mail_queued();
    Ok(())
}

/// Drops a message that ran out of attempts, along with its encrypted content.
pub fn discard(db: &Store, actor: &str, id: &str) -> Result<()> {
    change_failed(db, actor, id, "mail.discarded", || {
        db.platform.exec("DELETE FROM outbox WHERE id=?", [id])
    })
}
