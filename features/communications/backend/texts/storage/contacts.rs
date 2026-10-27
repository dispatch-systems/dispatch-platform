//! The names someone at the DSP saved for numbers that text it, shared at the DSP, and the
//! numbers blocked; each noted as a change to the number's conversation.
use crate::backend::changes::changed_conversation;
use dispatch_core::{
    Result,
    db::{Db, Store, now},
};
use rusqlite::params;
use std::collections::BTreeSet;

/// What texting reads and writes of the names and blocks the DSP keeps for numbers.
pub trait ContactStore {
    /// Every contact the DSP saved: each number with its name.
    fn communications_contacts(&self, dsp: &str) -> Result<Vec<(String, String)>>;
    /// Saves `name` for the number, as the member `by`.
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<()>;
    /// Every number blocked.
    fn communications_blocked(&self, dsp: &str) -> Result<BTreeSet<String>>;
    /// Blocks the number, as the member `by`, or unblocks it.
    fn block_communications_number(&self, dsp: &str, number: &str, by: Option<&str>) -> Result<()>;
}

impl ContactStore for Store {
    fn communications_contacts(&self, dsp: &str) -> Result<Vec<(String, String)>> {
        self.dsp(dsp)?.query_as(
            "SELECT number,name FROM communications_contacts ORDER BY number",
            [],
        )
    }
    fn save_communications_contact(
        &self,
        dsp: &str,
        number: &str,
        name: &str,
        by: &str,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            db.exec(
                "INSERT INTO communications_contacts (number,name,saved_by,saved_at) \
                 VALUES (?,?,?,?) ON CONFLICT(number) DO UPDATE SET name=excluded.name,\
                 saved_by=excluded.saved_by,saved_at=excluded.saved_at",
                params![number, name, by, now()],
            )?;
            changed_number(&db, number)
        })
    }
    fn communications_blocked(&self, dsp: &str) -> Result<BTreeSet<String>> {
        let blocked: Vec<(String,)> = self
            .dsp(dsp)?
            .query_as("SELECT number FROM communications_blocked", [])?;
        Ok(blocked.into_iter().map(|(number,)| number).collect())
    }
    fn block_communications_number(&self, dsp: &str, number: &str, by: Option<&str>) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            match by {
                Some(by) => db.exec(
                    "INSERT OR IGNORE INTO communications_blocked (number,blocked_by,blocked_at) \
                     VALUES (?,?,?)",
                    params![number, by, now()],
                )?,
                None => db.exec(
                    "DELETE FROM communications_blocked WHERE number=?",
                    [number],
                )?,
            };
            changed_number(&db, number)
        })
    }
}

/// Notes that the number's conversation changed, as a name saved for it or its block, for the
/// pages waiting on changes.
fn changed_number(db: &Db, number: &str) -> Result<()> {
    let found: Option<(String,)> = db.one_as(
        "SELECT id FROM communications_text_conversations \
         WHERE kind='number' AND number=? AND driver_id IS NULL",
        [number],
    )?;
    match found {
        Some((id,)) => changed_conversation(db, &id),
        None => Ok(()),
    }
}
