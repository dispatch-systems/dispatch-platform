//! The names someone at the DSP saved for numbers that text one of its lines, shared at the DSP,
//! and the numbers blocked on each, each line's its own: Dispatch Phone's (none), or another
//! feature's. Each noted as a change to the number's conversation on that line.
use crate::backend::changes::changed_conversation;
use dispatch_core::{
    Result,
    db::{Db, Store, now},
};
use rusqlite::params;
use std::collections::BTreeSet;

/// What texting reads and writes of the names and blocks the DSP keeps for numbers, by line.
pub trait ContactStore {
    /// Every contact the line `line` saved: each number with its name.
    fn communications_contacts(
        &self,
        dsp: &str,
        line: Option<&str>,
    ) -> Result<Vec<(String, String)>>;
    /// Saves `name` for the number on the line `line`, as the member `by`: the name saved before,
    /// if the name changed, which is none within for a number named for the first time.
    fn save_communications_contact(
        &self,
        dsp: &str,
        line: Option<&str>,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<Option<Option<String>>>;
    /// Notes that the number's conversation on Dispatch Phone's own line changed as who it's with
    /// did, as the number being added to a person or taken off them, for the pages waiting on
    /// changes.
    fn communications_number_changed(&self, dsp: &str, number: &str) -> Result<()>;
    /// Every number the line `line` blocked.
    fn communications_blocked(&self, dsp: &str, line: Option<&str>) -> Result<BTreeSet<String>>;
    /// Blocks the number on the line `line`, as the member `by`, or unblocks it: whether it was
    /// blocked or unblocked, as it wasn't already.
    fn block_communications_number(
        &self,
        dsp: &str,
        line: Option<&str>,
        number: &str,
        by: Option<&str>,
    ) -> Result<bool>;
}

/// The names the line `?1` saved, each number's with who saved it and when: Dispatch Phone's own
/// (none) kept as they always were, by the number alone, which the release before the lines reads
/// and writes; another line's beside them, by its line and the number.
const CONTACTS: &str = "(SELECT number,name,saved_by,saved_at FROM communications_contacts \
                        WHERE ?1 IS NULL UNION ALL SELECT number,name,saved_by,saved_at \
                        FROM communications_line_contacts WHERE line=?1)";
/// The numbers the line `?1` blocked, kept as `CONTACTS` are.
pub(super) const BLOCKED: &str = "(SELECT number FROM communications_blocked WHERE ?1 IS NULL \
                                  UNION ALL SELECT number FROM communications_line_blocked \
                                  WHERE line=?1)";

impl ContactStore for Store {
    fn communications_contacts(
        &self,
        dsp: &str,
        line: Option<&str>,
    ) -> Result<Vec<(String, String)>> {
        self.dsp(dsp)?.query_as(
            &format!("SELECT number,name FROM {CONTACTS} ORDER BY number"),
            [line],
        )
    }
    fn save_communications_contact(
        &self,
        dsp: &str,
        line: Option<&str>,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<Option<Option<String>>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let before: Option<(String,)> = db.one_as(
                &format!("SELECT name FROM {CONTACTS} WHERE number=?2"),
                params![line, number],
            )?;
            let before = before.map(|(name,)| name);
            if before.as_deref() == Some(name) {
                return Ok(None);
            }
            match line {
                None => db.exec(
                    "INSERT INTO communications_contacts (number,name,saved_by,saved_at) \
                     VALUES (?,?,?,?) ON CONFLICT(number) DO UPDATE SET name=excluded.name,\
                     saved_by=excluded.saved_by,saved_at=excluded.saved_at",
                    params![number, name, by, now()],
                )?,
                Some(line) => db.exec(
                    "INSERT INTO communications_line_contacts \
                     (line,number,name,saved_by,saved_at) VALUES (?,?,?,?,?) \
                     ON CONFLICT(line,number) DO UPDATE SET name=excluded.name,\
                     saved_by=excluded.saved_by,saved_at=excluded.saved_at",
                    params![line, number, name, by, now()],
                )?,
            };
            changed_number(&db, line, number)?;
            Ok(Some(before))
        })
    }
    fn communications_number_changed(&self, dsp: &str, number: &str) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| changed_number(&db, None, number))
    }
    fn communications_blocked(&self, dsp: &str, line: Option<&str>) -> Result<BTreeSet<String>> {
        let blocked: Vec<(String,)> = self
            .dsp(dsp)?
            .query_as(&format!("SELECT number FROM {BLOCKED}"), [line])?;
        Ok(blocked.into_iter().map(|(number,)| number).collect())
    }
    fn block_communications_number(
        &self,
        dsp: &str,
        line: Option<&str>,
        number: &str,
        by: Option<&str>,
    ) -> Result<bool> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let changed = match (line, by) {
                (None, Some(by)) => db.exec(
                    "INSERT OR IGNORE INTO communications_blocked (number,blocked_by,blocked_at) \
                     VALUES (?,?,?)",
                    params![number, by, now()],
                )?,
                (None, None) => db.exec(
                    "DELETE FROM communications_blocked WHERE number=?",
                    [number],
                )?,
                (Some(line), Some(by)) => db.exec(
                    "INSERT OR IGNORE INTO communications_line_blocked \
                     (line,number,blocked_by,blocked_at) VALUES (?,?,?,?)",
                    params![line, number, by, now()],
                )?,
                (Some(line), None) => db.exec(
                    "DELETE FROM communications_line_blocked WHERE line=? AND number=?",
                    [line, number],
                )?,
            };
            if changed > 0 {
                changed_number(&db, line, number)?;
            }
            Ok(changed > 0)
        })
    }
}

/// Notes that the number's conversation on the line `line` changed, as a name saved for it or its
/// block, for the pages waiting on changes.
fn changed_number(db: &Db, line: Option<&str>, number: &str) -> Result<()> {
    let found: Option<(String,)> = db.one_as(
        "SELECT id FROM communications_text_conversations \
         WHERE kind='number' AND number=? AND line IS ? AND driver_id IS NULL",
        params![number, line],
    )?;
    match found {
        Some((id,)) => changed_conversation(db, &id),
        None => Ok(()),
    }
}
