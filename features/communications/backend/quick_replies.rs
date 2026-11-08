//! The DSP's quick replies: short answers anyone at the DSP who uses Communications saves, put in
//! what they write with a press. One list for the DSP, kept in its own settings and edited whole
//! on Settings' Communications page, whose section Communications' own component draws, as a list
//! of texts is none of Settings' kinds of value. A save goes on the revision the list was read
//! at, so one made over another session's since is refused, as Settings refuses one.
use crate::{api::types::QuickReplies, backend::logged::together};
use dispatch_core::{
    Result,
    db::{AuditChange, Db, Store},
    ensure,
    manifest::{Section, section},
};
use serde_json::json;

/// Where they're edited: on Communications' own page of Settings, by anyone who uses it.
pub const QUICK_REPLIES: Section = section(
    "communications.quick_replies",
    "Quick replies",
    "communications.use",
    &[],
)
.custom()
.keywords(&["saved replies", "canned", "templates"]);

/// The most a DSP keeps.
pub const MOST: usize = 30;
/// The most characters one holds.
pub const LONGEST: usize = 500;
/// Where the DSP's settings keep them, with their revision: none saved yet is revision 0.
const KEPT: &str = "communications.quick_replies";

/// The DSP's quick replies, in their order, and their revision.
pub fn quick_replies(db: &Store, dsp: &str) -> Result<QuickReplies> {
    kept(&*db.dsp(dsp)?)
}
fn kept(db: &Db) -> Result<QuickReplies> {
    Ok(serde_json::from_value(db.setting(KEPT, json!({}))?)?)
}

/// Keeps `replies` as the DSP's, in their order, each trimmed, as the member `by`: refused past
/// `MOST`, one empty or past `LONGEST`, or once another save passed `revision`. A save that changed
/// the list is noted in the audit log with it, with each reply added and removed, each by its
/// first hundred characters.
pub fn save_quick_replies(
    db: &Store,
    dsp: &str,
    replies: &[String],
    revision: i64,
    by: &str,
) -> Result<QuickReplies> {
    let replies: Vec<String> = replies
        .iter()
        .map(|reply| reply.trim().to_owned())
        .collect();
    ensure(
        replies.len() <= MOST
            && replies
                .iter()
                .all(|reply| !reply.is_empty() && reply.chars().count() <= LONGEST),
        "invalid_input",
        400,
    )?;
    together(db, dsp, || {
        let people = db.dsp(dsp)?;
        let before = kept(&people)?;
        ensure(
            before.revision == revision,
            "settings_changed_reload_before_saving",
            409,
        )?;
        let saved = QuickReplies {
            replies,
            revision: revision + 1,
        };
        people.set(KEPT, &serde_json::to_value(&saved)?)?;
        if saved.replies != before.replies {
            let removed = missing(&before.replies, &saved.replies)
                .into_iter()
                .map(|reply| ("reply", Some(logged(&reply)), None));
            let added = missing(&saved.replies, &before.replies)
                .into_iter()
                .map(|reply| ("reply", None, Some(logged(&reply))));
            let changes: Vec<AuditChange> = removed.chain(added).collect();
            db.audit_with(
                Some(by),
                Some(dsp),
                "communications.quick_replies_saved",
                "",
                None,
                &changes,
            )?;
        }
        Ok(saved)
    })
}

/// How long a reply reads in the audit log at most, in characters: its start, enough to tell it.
const LOGGED: usize = 100;
/// A reply as the audit log keeps it: its first `LOGGED` characters, and an ellipsis past them.
fn logged(reply: &str) -> String {
    match reply.char_indices().nth(LOGGED) {
        Some((end, _)) => format!("{}…", &reply[..end]),
        None => reply.to_owned(),
    }
}

/// Each of `replies` that `others` hasn't, once, in their order.
fn missing(replies: &[String], others: &[String]) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for reply in replies {
        if !others.contains(reply) && !missing.contains(reply) {
            missing.push(reply.clone());
        }
    }
    missing
}

#[cfg(test)]
#[path = "../tests/backend/quick_replies.rs"]
mod tests;
