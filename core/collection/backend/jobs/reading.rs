//! Job history, active-job lookups and batched public projections.
use crate::{
    Error, Result,
    collection::api::jobs::{JobKind, JobRow, PublicJob},
    db::Store,
    job_statuses,
};
use rusqlite::params;
use std::collections::HashMap;

// Each query that reads, counts or runs a job takes `JobKind::known()`, so that none
// reaches a job of a kind this release doesn't run.
const RECENT: &str = "SELECT * FROM jobs WHERE kind IN (SELECT value FROM json_each(?)) \
    ORDER BY created_at DESC LIMIT 200";
const RECENT_FOR_DSP: &str = "SELECT * FROM jobs WHERE dsp_id=? \
    AND kind IN (SELECT value FROM json_each(?)) ORDER BY created_at DESC LIMIT 200";
const RECENT_OF_KIND: &str =
    "SELECT * FROM jobs WHERE dsp_id=? AND kind=? ORDER BY created_at DESC LIMIT 200";
const RECENT_IN: &str = "SELECT * FROM jobs WHERE dsp_id=? \
    AND kind IN (SELECT value FROM json_each(?)) ORDER BY created_at DESC LIMIT 200";
const DSP_NAMES: &str = "SELECT id,name FROM dsps WHERE id IN (SELECT value FROM json_each(?))";
const METRICS: &str = "SELECT job_id,metrics FROM job_metrics \
    WHERE job_id IN (SELECT value FROM json_each(?)) ORDER BY attempt";
const JOB: &str = "SELECT * FROM jobs WHERE id=? AND (? IS NULL OR dsp_id=?) \
    AND kind IN (SELECT value FROM json_each(?))";
const ACTIVE_OF_KIND: &str = concat!(
    "SELECT * FROM jobs WHERE dsp_id=? AND kind=? AND status IN ",
    job_statuses!(active),
    " ORDER BY created_at DESC LIMIT 1"
);

impl Store {
    pub fn public_job(&self, row: JobRow) -> Result<PublicJob> {
        let name = self.find_dsp(&row.dsp_id)?.name;
        let metrics = self.metrics(&row.id)?;
        PublicJob::new(row, name, metrics)
    }
    pub fn recent_jobs(&self, id: Option<&str>) -> Result<Vec<PublicJob>> {
        let known = JobKind::known()?;
        let rows: Vec<JobRow> = match id {
            Some(id) => self.jobs.query_as(RECENT_FOR_DSP, [id, &known])?,
            None => self.jobs.query_as(RECENT, [known])?,
        };
        self.public_jobs(rows)
    }
    /// A DSP's recent jobs of one kind, however many of other kinds came since.
    pub fn recent_jobs_of(&self, id: &str, kind: &str) -> Result<Vec<PublicJob>> {
        self.public_jobs(self.jobs.query_as(RECENT_OF_KIND, [id, kind])?)
    }
    /// A DSP's recent jobs of these kinds, however many of others came since.
    pub fn recent_jobs_in(&self, id: &str, kinds: &[&str]) -> Result<Vec<PublicJob>> {
        let kinds = serde_json::to_string(kinds)?;
        self.public_jobs(self.jobs.query_as(RECENT_IN, [id, kinds.as_str()])?)
    }
    /// A DSP's latest unfinished job of a kind.
    pub fn active_job_of(&self, id: &str, kind: &str) -> Result<Option<JobRow>> {
        self.jobs.one_as(ACTIVE_OF_KIND, params![id, kind])
    }
    /// The ids of a DSP's unfinished jobs of a kind.
    pub fn active_job_ids(&self, id: &str, kind: &str) -> Result<Vec<String>> {
        let jobs: Vec<(String,)> = self.jobs.query_as(
            concat!(
                "SELECT id FROM jobs WHERE dsp_id=? AND kind=? AND status IN ",
                job_statuses!(active)
            ),
            params![id, kind],
        )?;
        Ok(jobs.into_iter().map(|(job,)| job).collect())
    }
    /// Whether a DSP has an unfinished job of any of these kinds.
    pub fn any_active_job(&self, id: &str, kinds: &[&str]) -> Result<bool> {
        let kinds = serde_json::to_string(kinds)?;
        Ok(self
            .jobs
            .one(
                concat!(
                    "SELECT 1 FROM jobs WHERE dsp_id=? AND kind IN ",
                    "(SELECT value FROM json_each(?)) AND status IN ",
                    job_statuses!(active),
                    " LIMIT 1"
                ),
                [id, kinds.as_str()],
            )?
            .is_some())
    }
    /// A DSP's latest job of a kind that collects `date`, or that names no date at all.
    pub fn latest_job_for_date(&self, id: &str, kind: &str, date: &str) -> Result<Option<JobRow>> {
        self.jobs.one_as(
            "SELECT * FROM jobs WHERE dsp_id=? AND kind=? \
            AND (json_extract(request,'$.date')=? OR request='{}') ORDER BY created_at DESC LIMIT 1",
            params![id, kind, date],
        )
    }
    /// A DSP's latest job of a kind whose request holds each of these fields' values.
    pub fn latest_job_requesting(
        &self,
        id: &str,
        kind: &str,
        fields: &[(&'static str, &str)],
    ) -> Result<Option<JobRow>> {
        let mut sql = "SELECT * FROM jobs WHERE dsp_id=? AND kind=?".to_owned();
        for (field, _) in fields {
            // Field names are the code's own, never a caller's input.
            debug_assert!(
                field
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            );
            sql.push_str(&format!(" AND json_extract(request,'$.{field}')=?"));
        }
        sql.push_str(" ORDER BY created_at DESC,rowid DESC LIMIT 1");
        let values = [id, kind]
            .into_iter()
            .chain(fields.iter().map(|(_, value)| *value));
        self.jobs.one_as(&sql, rusqlite::params_from_iter(values))
    }
    /// The jobs a DSP queued under keys that begin with `prefix`, as one request queues a
    /// batch: those of the `first` kind first, then by key.
    pub fn jobs_keyed(&self, id: &str, prefix: &str, first: &str) -> Result<Vec<JobRow>> {
        self.jobs.query_as(
            "SELECT * FROM jobs WHERE dsp_id=? AND substr(idempotency_key,1,?)=? \
            AND kind IN (SELECT value FROM json_each(?)) \
            ORDER BY CASE kind WHEN ? THEN 0 ELSE 1 END,idempotency_key",
            params![
                id,
                prefix.chars().count() as i64,
                prefix,
                JobKind::known()?,
                first
            ],
        )
    }
    /// A DSP's latest job of a kind queued under a key that begins with `prefix` that
    /// failed, or else was cancelled.
    pub fn stopped_job_keyed(&self, id: &str, kind: &str, prefix: &str) -> Result<Option<JobRow>> {
        self.jobs.one_as(
            "SELECT * FROM jobs WHERE dsp_id=? AND kind=? \
            AND substr(idempotency_key,1,?)=? AND status IN ('failed','cancelled') \
            ORDER BY CASE status WHEN 'failed' THEN 0 ELSE 1 END,created_at DESC LIMIT 1",
            params![id, kind, prefix.chars().count() as i64, prefix],
        )
    }
    fn public_jobs(&self, rows: Vec<JobRow>) -> Result<Vec<PublicJob>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(&rows.iter().map(|r| &r.id).collect::<Vec<_>>())?;
        let dsps = serde_json::to_string(&rows.iter().map(|r| &r.dsp_id).collect::<Vec<_>>())?;
        let names: HashMap<String, String> = self
            .platform
            .query_as::<(String, String)>(DSP_NAMES, [dsps])?
            .into_iter()
            .collect();
        let mut metrics: HashMap<String, Vec<crate::collection::api::metrics::JobMetrics>> =
            HashMap::new();
        for (job, stored) in self.jobs.query_as::<(String, String)>(METRICS, [ids])? {
            metrics
                .entry(job)
                .or_default()
                .push(serde_json::from_str(&stored)?);
        }
        rows.into_iter()
            .map(|row| {
                let name = names
                    .get(&row.dsp_id)
                    .ok_or_else(|| Error::new("dsp_not_found", 404))?;
                let metrics = metrics.remove(&row.id).unwrap_or_default();
                PublicJob::new(row, name.clone(), metrics)
            })
            .collect()
    }
    pub fn job_row(&self, id: &str, dsp: Option<&str>) -> Result<JobRow> {
        self.jobs
            .one_as(JOB, params![id, dsp, dsp, JobKind::known()?])?
            .ok_or_else(|| Error::new("job_not_found", 404))
    }
}
