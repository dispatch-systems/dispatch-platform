//! Each phone's contacts, for a line that keeps its contacts on its phones (every line but
//! Dispatch Phone's own): each number the phone's contacts hold, by its name there, as the phone
//! says it; and each name someone at the DSP saved, waiting on each of the line's phones until it
//! says it saved it. Which name a line shows for a number is `LINE_CONTACTS`'.
use dispatch_core::{
    Result,
    db::{Db, Store},
    ensure,
};
use rusqlite::params;

/// How long a name handed to a phone to save waits for it to say it saved it before it's handed
/// again, as a phone that can't save contacts yet never says it.
pub const SAVE_WAIT_MS: i64 = 40 * 1000;
/// The most names handed to a phone to save at once.
pub const MOST_CONTACTS: usize = 100;

/// The name each number has on the line `?1`, by its feature's name, and when it last changed: the
/// one of its phones, not removed, whose name for it changed last.
pub(crate) const LINE_CONTACTS: &str = "(SELECT c.number,c.name,max(c.changed_at) AS changed_at \
     FROM phones_contacts c JOIN phones_devices d ON d.id=c.device_id \
     WHERE d.line=?1 AND d.removed_at IS NULL GROUP BY c.number)";

/// A name saved for a number, for a phone to save: when it was saved (`at`) is how the phone says
/// it saved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saving {
    pub number: String,
    pub name: String,
    pub at: i64,
}

/// What changed of a phone's contacts, as it says it: the names it saved as handed, by their number
/// and when each was saved; each number's name as its contacts now have it; and the numbers they
/// hold no more.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ContactsReport {
    pub saved: Vec<(String, i64)>,
    pub changed: Vec<(String, String)>,
    pub removed: Vec<String>,
}

/// What the phone side keeps of each phone's contacts.
pub trait PhoneContactStore {
    /// Saves `name` for `number` on each phone of the line `line`, not removed, for each to save:
    /// the name the line showed for it before, if the name changed, which is none within for a
    /// number named for the first time, and the phones it's for. Refused as `phones_none` while
    /// the line has none.
    fn phones_save_contact(
        &self,
        dsp: &str,
        line: &str,
        number: &str,
        name: &str,
        at: i64,
    ) -> Result<Option<(Option<String>, Vec<String>)>>;
    /// The names the phone `device` has yet to say it saved, oldest first, at most
    /// `MOST_CONTACTS`, but those handed after `since`.
    fn phones_contact_savings(&self, dsp: &str, device: &str, since: i64) -> Result<Vec<Saving>>;
    /// Notes the names in `savings` handed to the phone `device` at `at`.
    fn phones_contacts_handed(
        &self,
        dsp: &str,
        device: &str,
        savings: &[Saving],
        at: i64,
    ) -> Result<()>;
    /// Keeps what the phone `device` says changed of its contacts, at `at`: the numbers whose name
    /// on it changed.
    fn phones_contacts_reported(
        &self,
        dsp: &str,
        device: &str,
        report: &ContactsReport,
        at: i64,
    ) -> Result<Vec<String>>;
}

impl PhoneContactStore for Store {
    fn phones_save_contact(
        &self,
        dsp: &str,
        line: &str,
        number: &str,
        name: &str,
        at: i64,
    ) -> Result<Option<(Option<String>, Vec<String>)>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            let phones: Vec<(String,)> = db.query_as(
                "SELECT id FROM phones_devices WHERE line=? AND removed_at IS NULL",
                [line],
            )?;
            ensure(!phones.is_empty(), "phones_none", 409)?;
            let before: Option<(String,)> = db.one_as(
                &format!("SELECT name FROM {LINE_CONTACTS} WHERE number=?2"),
                params![line, number],
            )?;
            let before = before.map(|(name,)| name);
            if before.as_deref() == Some(name) {
                return Ok(None);
            }
            let phones: Vec<String> = phones.into_iter().map(|(id,)| id).collect();
            for phone in &phones {
                db.exec(
                    "INSERT INTO phones_contacts (device_id,number,name,changed_at,saved_at) \
                     VALUES (?1,?2,?3,?4,?4) ON CONFLICT(device_id,number) DO UPDATE SET \
                     name=excluded.name,changed_at=?4,saved_at=?4,handed_at=NULL",
                    params![phone, number, name, at],
                )?;
            }
            Ok(Some((before, phones)))
        })
    }
    fn phones_contact_savings(&self, dsp: &str, device: &str, since: i64) -> Result<Vec<Saving>> {
        let rows: Vec<(String, String, i64)> = self.dsp(dsp)?.query_as(
            "SELECT number,name,saved_at FROM phones_contacts \
             WHERE device_id=? AND saved_at IS NOT NULL AND coalesce(handed_at,0)<=? \
             ORDER BY saved_at,number LIMIT ?",
            params![device, since, MOST_CONTACTS as i64],
        )?;
        Ok(rows
            .into_iter()
            .map(|(number, name, at)| Saving { number, name, at })
            .collect())
    }
    fn phones_contacts_handed(
        &self,
        dsp: &str,
        device: &str,
        savings: &[Saving],
        at: i64,
    ) -> Result<()> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            for saving in savings {
                db.exec(
                    "UPDATE phones_contacts SET handed_at=? \
                     WHERE device_id=? AND number=? AND saved_at=?",
                    params![at, device, saving.number, saving.at],
                )?;
            }
            Ok(())
        })
    }
    fn phones_contacts_reported(
        &self,
        dsp: &str,
        device: &str,
        report: &ContactsReport,
        at: i64,
    ) -> Result<Vec<String>> {
        let db = self.dsp(dsp)?;
        db.transaction(|| {
            for (number, saved_at) in &report.saved {
                db.exec(
                    "UPDATE phones_contacts SET saved_at=NULL,handed_at=NULL \
                     WHERE device_id=? AND number=? AND saved_at=?",
                    params![device, number, saved_at],
                )?;
            }
            let mut changed = Vec::new();
            for (number, name) in &report.changed {
                // Its name as the phone has it, over one saved before; unchanged, nothing moves.
                let moved = db.exec(
                    "INSERT INTO phones_contacts (device_id,number,name,changed_at) \
                     VALUES (?1,?2,?3,?4) ON CONFLICT(device_id,number) DO UPDATE SET \
                     name=excluded.name,changed_at=?4,saved_at=NULL,handed_at=NULL \
                     WHERE name IS NOT excluded.name OR saved_at IS NOT NULL",
                    params![device, number, name, at],
                )?;
                if moved > 0 {
                    changed.push(number.clone());
                }
            }
            for number in &report.removed {
                if db.exec(
                    "DELETE FROM phones_contacts WHERE device_id=? AND number=?",
                    params![device, number],
                )? > 0
                {
                    changed.push(number.clone());
                }
            }
            Ok(changed)
        })
    }
}

/// Hands the phone `device`, just paired to the line `line`, the names its line shows, as names
/// saved for it to save, as they last changed, so a line's phones start alike. Inside its pairing.
pub(super) fn hand_line(db: &Db, line: &str, device: &str) -> Result<()> {
    db.exec(
        &format!(
            "INSERT INTO phones_contacts (device_id,number,name,changed_at,saved_at) \
             SELECT ?2,number,name,changed_at,changed_at FROM {LINE_CONTACTS} WHERE true \
             ON CONFLICT DO NOTHING"
        ),
        params![line, device],
    )?;
    Ok(())
}

/// Lets go of the phone `device`'s contacts, as it's removed: its line shows them no more, nor
/// does any phone save them. Inside its removal.
pub(super) fn forget(db: &Db, device: &str) -> Result<()> {
    db.exec("DELETE FROM phones_contacts WHERE device_id=?", [device])?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/backend/phone/storage/contacts.rs"]
mod tests;
