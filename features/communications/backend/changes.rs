//! Each change to a conversation of either kind, or a group, in order (`communications_changes`),
//! so a page waiting on them reads only those it hasn't seen (`live`'s `updates`). The latest
//! thousand are kept; a page further behind reads afresh.
use dispatch_core::{
    Result,
    db::{Db, Store},
};

/// How many changes are kept for the pages waiting on them: one further behind reads afresh.
const CHANGES_KEPT: i64 = 1000;

/// What Communications reads of the changes to its conversations.
pub trait ChangeStore {
    /// The latest change to any conversation: 0 before the first.
    fn communications_revision(&self, dsp: &str) -> Result<i64>;
    /// The conversations changed since the change `after`, and the latest change: none for
    /// changes no longer kept, which reading afresh answers.
    fn communications_changed_since(
        &self,
        dsp: &str,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)>;
}

impl ChangeStore for Store {
    fn communications_revision(&self, dsp: &str) -> Result<i64> {
        let db = self.dsp(dsp)?;
        revision(&db)
    }
    fn communications_changed_since(
        &self,
        dsp: &str,
        after: i64,
    ) -> Result<(Option<Vec<String>>, i64)> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let latest = revision(&db)?;
            let first = db.count(
                "SELECT coalesce(min(revision),0) FROM communications_changes",
                [],
            )?;
            // Changes since one no longer kept, or one never made, are read afresh.
            if after > latest || after < first - 1 {
                return Ok((None, latest));
            }
            let changed: Vec<(String,)> = db.query_as(
                "SELECT DISTINCT conversation_id FROM communications_changes WHERE revision>?",
                [after],
            )?;
            Ok((Some(changed.into_iter().map(|(id,)| id).collect()), latest))
        })
    }
}

/// Notes the conversation, of either kind, or the group (by its own ID, which the column holds as
/// well), changed, for the pages waiting on changes, keeping the latest thousand.
pub(crate) fn changed_conversation(db: &Db, conversation: &str) -> Result<()> {
    db.exec(
        "INSERT INTO communications_changes (conversation_id) VALUES (?)",
        [conversation],
    )?;
    db.exec(
        "DELETE FROM communications_changes WHERE revision<=\
         (SELECT max(revision) FROM communications_changes)-?",
        [CHANGES_KEPT],
    )?;
    Ok(())
}

fn revision(db: &Db) -> Result<i64> {
    db.count(
        "SELECT coalesce(max(revision),0) FROM communications_changes",
        [],
    )
}
