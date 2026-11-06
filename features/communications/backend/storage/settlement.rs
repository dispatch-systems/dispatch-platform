//! Shared conversation and group settlement, within the caller's transaction.
use crate::backend::{
    changes::changed_conversation, groups::storage::settle_group, model::ConversationState,
};
use dispatch_core::{
    Error, Result,
    db::{Db, now},
};
use rusqlite::params;

/// Marks the conversation done as the member `by`, covering what was written up to `seen`, the
/// latest message they were shown, when it was written and its `ts`, else up to now: never less
/// than a Done before covered. A Done covers its millisecond (`done_at`) up to its `ts`
/// (`done_ts`), so one of the member's in that millisecond after it, kept before or after, still
/// waits; one with no `ts` covers all of it.
pub(crate) fn mark_done(
    db: &Db,
    conversation: &str,
    by: &str,
    seen: Option<(i64, &str)>,
) -> Result<()> {
    let at = now();
    let (until, ts) = match seen {
        Some((seen, ts)) if seen < at => (seen, Some(ts.to_owned())),
        _ => (at, None),
    };
    let (done_at, done_ts): (Option<i64>, Option<String>) = db
        .one_as(
            "SELECT done_at,done_ts FROM communications_conversations WHERE id=?",
            [conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    // How far a Done covers: to its millisecond, then all of it, else up to its `ts` in it.
    let reach = |at: Option<i64>, ts: &Option<String>| (at, ts.is_none(), ts.clone());
    if reach(Some(until), &ts) <= reach(done_at, &done_ts) {
        return Ok(());
    }
    db.exec(
        "UPDATE communications_conversations SET done_at=?,done_ts=?,done_by=? WHERE id=?",
        params![until, ts, by, conversation],
    )?;
    Ok(())
}
/// Keeps whether the conversation waits on an answer, and since when, from its messages and
/// when it was marked done, and the group it's listed under, and notes it changed, with each
/// group that changed with it. It waits while the member's latest message came after anyone at
/// the DSP answered them alone or marked it done: since the first of theirs after that. A
/// message to a group answers no one. It's listed under the group the DSP's latest message to
/// them went to, while all it waits on answers a message to a group: nothing they wrote before
/// one, since the DSP last wrote to them alone, still waits.
pub(crate) fn settle(db: &Db, conversation: &str) -> Result<()> {
    settle_with(db, conversation, Vec::new())
}
/// `settle`, with the groups `touched` settled again too, as those whose answers changed.
pub(crate) fn settle_with(db: &Db, conversation: &str, mut touched: Vec<String>) -> Result<()> {
    let (last_alone, done_at, was): (Option<i64>, Option<i64>, Option<String>) = db
        .one_as(
            "SELECT (SELECT max(at) FROM communications_messages \
             WHERE conversation_id=?1 AND direction='out' AND group_id IS NULL),\
             done_at,group_id FROM communications_conversations WHERE id=?1",
            [conversation],
        )?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    // What it waits on, if it waits: the first of the member's after the latest answer that the
    // latest Done didn't cover (`mark_done`), and whether it answers a group.
    let first: Option<(i64, Option<String>)> = db.one_as(
        "SELECT m.at,m.group_id FROM communications_messages m \
         JOIN communications_conversations c ON c.id=m.conversation_id \
         WHERE m.conversation_id=?1 AND m.direction='in' AND m.at>?2 \
         AND (m.at>coalesce(c.done_at,-1) OR (m.at=c.done_at AND m.ts>c.done_ts)) \
         ORDER BY m.at,m.ts LIMIT 1",
        params![conversation, last_alone.unwrap_or(-1)],
    )?;
    let state = if first.is_some() {
        ConversationState::Waiting
    } else if done_at > last_alone {
        ConversationState::Done
    } else {
        ConversationState::Responded
    };
    let since = first.as_ref().map(|(at, _)| *at);
    // Under the group the DSP last wrote to them in, while all it waits on answers a group.
    let latest: Option<(Option<String>,)> = db.one_as(
        "SELECT group_id FROM communications_messages WHERE conversation_id=? \
         AND direction='out' ORDER BY ts DESC LIMIT 1",
        [conversation],
    )?;
    let group = latest
        .and_then(|(group,)| group)
        .filter(|_| first.as_ref().is_none_or(|(_, answers)| answers.is_some()));
    db.exec(
        "UPDATE communications_conversations SET state=?2,waiting_since=?3,group_id=?4,\
         last_at=(SELECT max(at) FROM communications_messages WHERE conversation_id=?1) \
         WHERE id=?1",
        params![conversation, state, since, group],
    )?;
    changed_conversation(db, conversation)?;
    touched.extend(was.into_iter().chain(group));
    touched.sort();
    touched.dedup();
    for group in touched {
        settle_group(db, &group)?;
    }
    Ok(())
}
