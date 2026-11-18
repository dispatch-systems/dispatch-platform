//! Each change to a conversation of either kind, or a group, in order (`communications_changes`),
//! each of the line its conversation is of (none for Dispatch Phone's own, as every Slack
//! conversation and group is), so a page waiting on one line's reads only the changes it hasn't
//! seen of that line (`live`'s `updates`). The latest thousand of each line are kept; a page
//! further behind reads afresh.
use dispatch_core::{
    Result,
    db::{Db, Store},
};
use rusqlite::params;

/// How many changes of each line are kept for the pages waiting on them: one further behind reads
/// afresh.
const CHANGES_KEPT: i64 = 1000;

/// What Communications reads of the changes to its conversations.
pub trait ChangeStore {
    /// The latest change to any conversation of the line `line`, Dispatch Phone's own for none: 0
    /// before the first.
    fn communications_revision(&self, dsp: &str, line: Option<&str>) -> Result<i64>;
    /// The conversations of the line `line` changed since the change `after`, and its latest
    /// change: none for changes no longer kept, which reading afresh answers.
    fn communications_changed_since(
        &self,
        dsp: &str,
        line: Option<&str>,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)>;
}

impl ChangeStore for Store {
    fn communications_revision(&self, dsp: &str, line: Option<&str>) -> Result<i64> {
        let db = self.dsp(dsp)?;
        revision(&db, line)
    }
    fn communications_changed_since(
        &self,
        dsp: &str,
        line: Option<&str>,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let latest = revision(&db, line)?;
            let first = db.count(
                "SELECT coalesce(min(revision),0) FROM communications_changes WHERE line IS ?",
                [line],
            )?;
            // Changes since one no longer kept, or one never made, are read afresh.
            if after > latest || after < first - 1 {
                return Ok((None, latest));
            }
            let changed: Vec<(String,)> = db.query_as(
                "SELECT DISTINCT conversation_id FROM communications_changes \
                 WHERE line IS ? AND revision>?",
                params![line, after],
            )?;
            Ok((Some(changed.into_iter().map(|(id,)| id).collect()), latest))
        })
    }
}

/// Notes the conversation, of either kind, or the group (by its own ID, which the column holds as
/// well), changed, for the pages waiting on changes of its line, keeping that line's latest
/// thousand.
pub(crate) fn changed_conversation(db: &Db, conversation: &str) -> Result<()> {
    let line: Option<(Option<String>,)> = db.one_as(
        "SELECT line FROM communications_text_conversations WHERE id=?",
        [conversation],
    )?;
    let line = line.and_then(|(line,)| line);
    db.exec(
        "INSERT INTO communications_changes (conversation_id,line) VALUES (?,?)",
        params![conversation, line],
    )?;
    db.exec(
        "DELETE FROM communications_changes WHERE line IS ?1 AND revision<=(SELECT revision \
         FROM communications_changes WHERE line IS ?1 ORDER BY revision DESC LIMIT 1 OFFSET ?2)",
        params![line, CHANGES_KEPT],
    )?;
    Ok(())
}

fn revision(db: &Db, line: Option<&str>) -> Result<i64> {
    db.count(
        "SELECT coalesce(max(revision),0) FROM communications_changes WHERE line IS ?",
        [line],
    )
}
