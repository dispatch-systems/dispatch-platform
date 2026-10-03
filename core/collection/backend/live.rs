//! Validated, temporary driver results. Complete publications remain authoritative
//! for history; failed/cancelled/replaced attempts never overwrite them. Each collector
//! stages its own items; this is the store they are staged in and read from.
use super::{
    Error, Result,
    collectors::Provider,
    db::{self, Db, Store, s},
};
use rusqlite::params;
use serde_json::Value;
use std::collections::BTreeMap;

pub type LiveResults = Vec<(Value, Vec<Value>)>;

// No collection survives a restart, so neither do its live rows.
pub fn reset(db: &Db) -> Result<()> {
    db.exec("DELETE FROM collection_live_runs", [])?;
    Ok(())
}
impl Store {
    pub fn start_live(&self, job: &str, owner: &str, metadata: &Value) -> Result<()> {
        let dsp = self.guard(job, owner)?;
        let row = self.job_row(job, None)?;
        let db = self.collector(&dsp.id, row.provider())?;
        db.transaction(|| {
            // Each provider has one running collection per DSP. Discard any old attempt.
            db.exec("DELETE FROM collection_live_runs", [])?;
            db.exec(
                "INSERT INTO collection_live_runs VALUES (?,?,?)",
                params![job, owner, metadata.to_string()],
            )?;
            Ok(())
        })
    }
    pub fn live_results(&self, dsp: &str, provider: Provider, date: &str) -> Result<LiveResults> {
        Ok(self
            .live_results_range(dsp, provider, date, date)?
            .remove(date)
            .unwrap_or_default())
    }
    /// Reads each guarded live run once for a date range. Covered days retain
    /// their metadata even before their first item has been staged.
    pub fn live_results_range(
        &self,
        dsp: &str,
        provider: Provider,
        from: &str,
        to: &str,
    ) -> Result<BTreeMap<String, LiveResults>> {
        let parse = |date: &str| {
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| Error::new("invalid_date", 400))
        };
        let start = parse(from)?;
        let end = parse(to)?;
        let db = self.collector(dsp, provider)?;
        let mut out: BTreeMap<String, LiveResults> = BTreeMap::new();
        for (job, owner) in self.jobs.query_as::<(String, String)>(
            "SELECT id,lease_owner FROM jobs WHERE dsp_id=? AND kind=? \
            AND status='running' AND lease_until>?",
            params![dsp, provider.job_kind(), db::now()],
        )? {
            // Includes current connection revision, actor permissions and lease ownership.
            if self.guard(&job, &owner).is_err() {
                continue;
            }
            if let Some(run) = run_metadata(&db, &job, &owner)? {
                let metadata: Value = serde_json::from_str(&run)?;
                if to < s(&metadata, "from") || from > s(&metadata, "to") {
                    continue;
                }
                let mut items: BTreeMap<String, Vec<Value>> = BTreeMap::new();
                for (date, data) in db.query_as::<(String, String)>(
                    "SELECT date,data FROM collection_live_items WHERE job_id=? \
                     AND date BETWEEN ? AND ? ORDER BY date,item_key",
                    [&job, from, to],
                )? {
                    items
                        .entry(date)
                        .or_default()
                        .push(serde_json::from_str(&data)?);
                }
                let mut date = start;
                while date <= end {
                    let day = date.to_string();
                    if day.as_str() >= s(&metadata, "from") && day.as_str() <= s(&metadata, "to") {
                        out.entry(day.clone())
                            .or_default()
                            .push((metadata.clone(), items.remove(&day).unwrap_or_default()));
                    }
                    let Some(next) = date.succ_opt() else { break };
                    date = next;
                }
            }
        }
        Ok(out)
    }
    pub fn clear_live(&self, dsp: &str, provider: Provider, job: Option<&str>) -> Result<()> {
        self.collector(dsp, provider)?.exec(
            "DELETE FROM collection_live_runs WHERE (?1 IS NULL OR job_id=?1)",
            [job],
        )?;
        Ok(())
    }
}

/// A live run's metadata, while the run is still `owner`'s.
pub fn run_metadata(db: &Db, job: &str, owner: &str) -> Result<Option<String>> {
    let run: Option<(String,)> = db.one_as(
        "SELECT metadata FROM collection_live_runs WHERE job_id=? AND owner=?",
        [job, owner],
    )?;
    Ok(run.map(|(metadata,)| metadata))
}
/// Sets one field of a live run's metadata, while the run is still `owner`'s.
pub fn set_run_field(db: &Db, job: &str, owner: &str, field: &str, value: &Value) -> Result<()> {
    db.exec(
        "UPDATE collection_live_runs SET metadata=json_set(metadata,?1,json(?2)) \
        WHERE job_id=?3 AND owner=?4",
        params![format!("$.{field}"), value.to_string(), job, owner],
    )?;
    Ok(())
}
/// Whether the job's live run holds `value` in one field of its metadata, such as a token
/// only the attempt that started it knows.
pub fn run_marked(db: &Db, job: &str, field: &str, value: &str) -> Result<bool> {
    Ok(db
        .one(
            "SELECT 1 FROM collection_live_runs WHERE job_id=? AND json_extract(metadata,?)=?",
            params![job, format!("$.{field}"), value],
        )?
        .is_some())
}

/// Stages one validated item of a live run, while the run is still `owner`'s. A later
/// item with the same key and date replaces it.
pub fn stage_item(
    db: &Db,
    job: &str,
    owner: &str,
    key: &str,
    date: &str,
    data: &str,
) -> Result<()> {
    db.exec(
        "INSERT OR REPLACE INTO collection_live_items SELECT job_id,?1,?2,?3 FROM \
        collection_live_runs WHERE job_id=?4 AND owner=?5",
        params![key, date, data, job, owner],
    )?;
    Ok(())
}
