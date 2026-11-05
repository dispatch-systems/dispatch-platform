//! The DSP's quick replies: short answers anyone at the DSP who uses Communications saves, put in
//! what they write with a press. One list for the DSP, kept in its own settings and edited whole
//! on Settings' Communications page, whose section Communications' own component draws, as a list
//! of texts is none of Settings' kinds of value. A save goes on the revision the list was read
//! at, so one made over another session's since is refused, as Settings refuses one.
use crate::api::types::QuickReplies;
use dispatch_core::{
    Result,
    db::{Db, Store},
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

/// Keeps `replies` as the DSP's, in their order, each trimmed: refused past `MOST`, one empty or
/// past `LONGEST`, or once another save passed `revision`.
pub fn save_quick_replies(
    db: &Store,
    dsp: &str,
    replies: &[String],
    revision: i64,
) -> Result<QuickReplies> {
    let replies: Vec<String> = replies.iter().map(|reply| reply.trim().to_owned()).collect();
    ensure(
        replies.len() <= MOST
            && replies
                .iter()
                .all(|reply| !reply.is_empty() && reply.chars().count() <= LONGEST),
        "invalid_input",
        400,
    )?;
    let db = db.dsp(dsp)?;
    db.transaction(|| {
        ensure(
            kept(&db)?.revision == revision,
            "settings_changed_reload_before_saving",
            409,
        )?;
        let saved = QuickReplies {
            replies,
            revision: revision + 1,
        };
        db.set(KEPT, &serde_json::to_value(&saved)?)?;
        Ok(saved)
    })
}

#[cfg(test)]
#[path = "../tests/backend/quick_replies.rs"]
mod tests;
