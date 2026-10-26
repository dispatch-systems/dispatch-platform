//! What the server keeps of each DSP's reading, in the environment's jobs database: each read's
//! outcome and time, with counts and codes only (`live_reads`), and the days each reader
//! started and stopped there (`live_days`), which a restart reads to go on as it was.
use crate::{Result, db::Store, manifest::Counts};
use chrono::NaiveDate;
use rusqlite::params;
use serde_json::{Map, Value};

/// How long a DSP's reads and days are kept: two weeks.
const KEPT_FOR_MS: i64 = 14 * 24 * 60 * 60 * 1000;

/// One read, as its record keeps it.
pub(super) struct Read<'a> {
    pub reader: &'a str,
    pub dsp: &'a str,
    pub day: NaiveDate,
    /// When it began, once its turn came.
    pub at: i64,
    pub ms: i64,
    /// What it counted, kept; or why it wasn't, by its error's code.
    pub outcome: std::result::Result<Counts, String>,
}

impl Store {
    /// When `reader` last read for the DSP, whatever came of it.
    pub(super) fn last_read(&self, reader: &str, dsp: &str) -> Result<Option<i64>> {
        let last: Option<(Option<i64>,)> = self.jobs.one_as(
            "SELECT max(at) FROM live_reads WHERE reader=? AND dsp_id=?",
            [reader, dsp],
        )?;
        Ok(last.and_then(|(at,)| at))
    }
    pub(super) fn record_read(&self, read: &Read) -> Result<()> {
        let (outcome, code, counts) = match &read.outcome {
            Ok(counts) => {
                let counts: Map<String, Value> = counts
                    .iter()
                    .map(|(name, count)| ((*name).to_owned(), Value::from(*count)))
                    .collect();
                ("kept", None, Some(Value::Object(counts).to_string()))
            }
            Err(code) => ("failed", Some(code.as_str()), None),
        };
        self.jobs.exec(
            "INSERT INTO live_reads(reader,dsp_id,day,at,ms,outcome,code,counts) \
             VALUES (?,?,?,?,?,?,?,?)",
            params![
                read.reader,
                read.dsp,
                read.day.to_string(),
                read.at,
                read.ms,
                outcome,
                code,
                counts
            ],
        )?;
        Ok(())
    }
    /// The days `reader` started for the DSP and hasn't stopped, earliest first.
    pub(super) fn unstopped_days(&self, reader: &str, dsp: &str) -> Result<Vec<NaiveDate>> {
        let days: Vec<(String,)> = self.jobs.query_as(
            "SELECT day FROM live_days WHERE reader=? AND dsp_id=? AND stopped_at IS NULL \
             ORDER BY day",
            [reader, dsp],
        )?;
        Ok(days
            .into_iter()
            .filter_map(|(day,)| day.parse().ok())
            .collect())
    }
    /// The last day `reader` ended for the DSP, its stop begun or done, if one is still
    /// recorded.
    pub(super) fn last_ended(&self, reader: &str, dsp: &str) -> Result<Option<NaiveDate>> {
        let day: Option<(Option<String>,)> = self.jobs.one_as(
            "SELECT max(day) FROM live_days WHERE reader=? AND dsp_id=? \
             AND (stopping_at IS NOT NULL OR stopped_at IS NOT NULL)",
            [reader, dsp],
        )?;
        Ok(day.and_then(|(day,)| day?.parse().ok()))
    }
    /// Whether `reader` started `day` for the DSP, stopped since or not.
    pub(super) fn day_started(&self, reader: &str, dsp: &str, day: NaiveDate) -> Result<bool> {
        Ok(self
            .jobs
            .one_as::<(i64,)>(
                "SELECT 1 FROM live_days WHERE reader=? AND dsp_id=? AND day=?",
                params![reader, dsp, day.to_string()],
            )?
            .is_some())
    }
    /// Records that `day` reads for the DSP: started now, unless it started already. A day
    /// whose stop began never reads again (`Plan::step`).
    pub(super) fn keep_day(&self, reader: &str, dsp: &str, day: NaiveDate, at: i64) -> Result<()> {
        self.jobs.exec(
            "INSERT INTO live_days(reader,dsp_id,day,started_at) VALUES (?,?,?,?) \
             ON CONFLICT(reader,dsp_id,day) DO NOTHING",
            params![reader, dsp, day.to_string(), at],
        )?;
        Ok(())
    }
    /// Records that `day`'s stop began for the DSP, when it first did: from then it reads no
    /// more, and its stop is asked again until it's done.
    pub(super) fn begin_stop(
        &self,
        reader: &str,
        dsp: &str,
        day: NaiveDate,
        at: i64,
    ) -> Result<()> {
        self.jobs.exec(
            "UPDATE live_days SET stopping_at=? WHERE reader=? AND dsp_id=? AND day=? \
             AND stopping_at IS NULL",
            params![at, reader, dsp, day.to_string()],
        )?;
        Ok(())
    }
    /// Records that `day` stopped for the DSP.
    pub(super) fn stop_day(&self, reader: &str, dsp: &str, day: NaiveDate, at: i64) -> Result<()> {
        self.jobs.exec(
            "UPDATE live_days SET stopped_at=? WHERE reader=? AND dsp_id=? AND day=?",
            params![at, reader, dsp, day.to_string()],
        )?;
        Ok(())
    }
    /// Lets the DSP's reads from before the last two weeks go, and the days that stopped
    /// before then, whether any read was kept since or none.
    pub(super) fn prune_reads(&self, reader: &str, dsp: &str, at: i64) -> Result<()> {
        let before = at - KEPT_FOR_MS;
        self.jobs.transaction(|| {
            self.jobs.exec(
                "DELETE FROM live_reads WHERE reader=? AND dsp_id=? AND at<?",
                params![reader, dsp, before],
            )?;
            self.jobs.exec(
                "DELETE FROM live_days WHERE reader=? AND dsp_id=? AND stopped_at<?",
                params![reader, dsp, before],
            )?;
            Ok(())
        })
    }
}
