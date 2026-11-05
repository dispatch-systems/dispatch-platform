//! The days texting keeps of: each day whose texts are still to be filed again, as filing them
//! again failed (`communications_refiling`); and each day live route data held, with when it was
//! first seen not to, at the stop (`communications_live_days`).
use crate::backend::texts::model::LiveKept;
use dispatch_core::{
    Result,
    db::{Db, Store, migrations::add_column},
};
use rusqlite::params;

/// What texting reads and writes of the days it keeps.
pub trait DayStore {
    /// Notes `day`, YYYY-MM-DD in the DSP's time, as still to be filed again, at `at`: noted
    /// already, it keeps when it first was.
    fn note_communications_refiling(&self, dsp: &str, day: &str, at: i64) -> Result<()>;
    /// The days still to be filed again, the earliest day first, the earliest `most`.
    fn communications_refiling(&self, dsp: &str, most: usize) -> Result<Vec<String>>;
    /// Lets go of `day` as still to be filed again, once it's filed.
    fn refiled_communications_day(&self, dsp: &str, day: &str) -> Result<()>;
    /// The latest day live route data was seen holding, and since when it hasn't.
    fn communications_live_day(&self, dsp: &str) -> Result<Option<LiveKept>>;
    /// Keeps that live route data holds `day`: again, too, once it was seen not to.
    fn live_communications_day(&self, dsp: &str, day: &str) -> Result<()>;
    /// Keeps that live route data held `day` until `at`, unless it was kept ending already.
    fn ended_communications_day(&self, dsp: &str, day: &str, at: i64) -> Result<()>;
    /// Keeps that the pages heard the work conversations stop waiting after `day`'s stop.
    fn over_communications_day(&self, dsp: &str, day: &str) -> Result<()>;
}

impl DayStore for Store {
    fn note_communications_refiling(&self, dsp: &str, day: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT OR IGNORE INTO communications_refiling (day,noted_at) VALUES (?,?)",
            params![day, at],
        )?;
        Ok(())
    }
    fn communications_refiling(&self, dsp: &str, most: usize) -> Result<Vec<String>> {
        let days: Vec<(String,)> = self.dsp(dsp)?.query_as(
            "SELECT day FROM communications_refiling ORDER BY day LIMIT ?",
            [most as i64],
        )?;
        Ok(days.into_iter().map(|(day,)| day).collect())
    }
    fn refiled_communications_day(&self, dsp: &str, day: &str) -> Result<()> {
        self.dsp(dsp)?
            .exec("DELETE FROM communications_refiling WHERE day=?", [day])?;
        Ok(())
    }
    fn communications_live_day(&self, dsp: &str) -> Result<Option<LiveKept>> {
        Ok(self
            .dsp(dsp)?
            .query_as(
                "SELECT day,ended_at,over FROM communications_live_days ORDER BY day DESC LIMIT 1",
                [],
            )?
            .pop())
    }
    fn live_communications_day(&self, dsp: &str, day: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "INSERT INTO communications_live_days (day) VALUES (?) \
             ON CONFLICT(day) DO UPDATE SET ended_at=NULL,over=0",
            [day],
        )?;
        Ok(())
    }
    fn ended_communications_day(&self, dsp: &str, day: &str, at: i64) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_live_days SET ended_at=? WHERE day=? AND ended_at IS NULL",
            params![at, day],
        )?;
        Ok(())
    }
    fn over_communications_day(&self, dsp: &str, day: &str) -> Result<()> {
        self.dsp(dsp)?.exec(
            "UPDATE communications_live_days SET over=1 WHERE day=?",
            [day],
        )?;
        Ok(())
    }
}

/// Communications' migration 14: the days still to be filed again, as filing them again failed
/// (`communications_refiling`: each day, YYYY-MM-DD in the DSP's time, and when it was first
/// noted); each day live route data was seen holding (`communications_live_days`: when it was
/// first seen not to, none while it does, and whether the pages heard the two hours after end,
/// `over`); and, on each text waiting to be filed, since when placing it failed
/// (`phones_texts.unfiled_since`). Each step is taken only if it wasn't.
pub(crate) fn add_days(db: &Db) -> Result<()> {
    db.exec(
        "CREATE TABLE IF NOT EXISTS communications_refiling (day TEXT PRIMARY KEY, \
         noted_at INTEGER NOT NULL)",
        [],
    )?;
    db.exec(
        "CREATE TABLE IF NOT EXISTS communications_live_days (day TEXT PRIMARY KEY, \
         ended_at INTEGER, over INTEGER NOT NULL DEFAULT 0)",
        [],
    )?;
    add_column(db, "phones_texts", "unfiled_since", "INTEGER")
}
