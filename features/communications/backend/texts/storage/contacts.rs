//! The names someone at the DSP saved for numbers that text one of its lines, shared at the DSP,
//! and the numbers blocked on each, each line's its own: Dispatch Phone's (none), or another
//! feature's, whose names are its phones' contacts (`phone::storage::PhoneContactStore`). Each
//! noted as a change to the number's conversation on that line.
use crate::backend::{changes::changed_conversation, phone::storage::LINE_CONTACTS};
use dispatch_core::{
    Result,
    db::{Db, Store, now},
};
use rusqlite::params;
use std::collections::BTreeSet;

/// What texting reads and writes of the names and blocks the DSP keeps for numbers, by line.
pub trait ContactStore {
    /// Every contact the line `line` shows: each number with its name.
    fn communications_contacts(
        &self,
        dsp: &str,
        line: Option<&str>,
    ) -> Result<Vec<(String, String)>>;
    /// Saves `name` for the number on Dispatch Phone's own line, as the member `by`: the name
    /// saved before, if the name changed, which is none within for a number named for the first
    /// time. Another line's are saved on its phones (`phones_save_contact`).
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<Option<Option<String>>>;
    /// Notes that the conversations of `numbers` on the line `line` changed as who they're with
    /// did, as a number being added to a person or taken off them, or its name on the line's
    /// phones changing, for the pages waiting on changes.
    fn communications_numbers_changed(
        &self,
        dsp: &str,
        line: Option<&str>,
        numbers: &[String],
    ) -> Result<()>;
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

/// The names the line `?1` shows: Dispatch Phone's own (none) kept as they always were, by the
/// number alone, which the release before the lines reads and writes; another line's, its
/// phones' (`LINE_CONTACTS`).
fn contacts() -> String {
    format!(
        "(SELECT number,name FROM communications_contacts WHERE ?1 IS NULL \
         UNION ALL SELECT number,name FROM {LINE_CONTACTS})"
    )
}
/// The numbers the line `?1` blocked: Dispatch Phone's own by the number alone, as its names are,
/// another line's by its line and the number.
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
            &format!("SELECT number,name FROM {} ORDER BY number", contacts()),
            [line],
        )
    }
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<Option<Option<String>>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let before: Option<(String,)> = db.one_as(
                "SELECT name FROM communications_contacts WHERE number=?",
                [number],
            )?;
            let before = before.map(|(name,)| name);
            if before.as_deref() == Some(name) {
                return Ok(None);
            }
            db.exec(
                "INSERT INTO communications_contacts (number,name,saved_by,saved_at) \
                 VALUES (?,?,?,?) ON CONFLICT(number) DO UPDATE SET name=excluded.name,\
                 saved_by=excluded.saved_by,saved_at=excluded.saved_at",
                params![number, name, by, now()],
            )?;
            changed_number(&db, None, number)?;
            Ok(Some(before))
        })
    }
    fn communications_numbers_changed(
        &self,
        dsp: &str,
        line: Option<&str>,
        numbers: &[String],
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            numbers
                .iter()
                .try_for_each(|number| changed_number(&db, line, number))
        })
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
