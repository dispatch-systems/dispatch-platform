use crate::collection::registry::Provider;
use crate::{
    Code, Error, Result,
    accounts::api::types::{Dsp, UserStatus},
    collection::{
        api::jobs::{ActiveJobStatus, JobKind, JobRow, JobStatus, PublicJob},
        metrics::Metrics,
    },
    db::{AuditChange, FromRow, Row, Store, iso, now},
    ensure,
    foundation::crypto,
    job_statuses,
};
use rusqlite::params;
use serde_json::Value;
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
const ACTIVE_COUNT: &str = concat!(
    "SELECT count(*) FROM jobs WHERE dsp_id=? AND kind IN (SELECT value FROM json_each(?)) \
     AND status IN ",
    job_statuses!(active)
);
const LEASED_COUNT: &str = concat!(
    "SELECT count(*) FROM jobs WHERE status IN ",
    job_statuses!(leased)
);
const INSERT: &str = "INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,\
    release,actor_id,connection_revision,idempotency_key,request) \
    VALUES (?,?,?,?,'queued',?,?,?,?,?,?,?)";
// The oldest queued job of the DSP that has waited longest since its last collection,
// among DSPs no worker is collecting for.
const CLAIMABLE: &str = concat!(
    "SELECT * FROM jobs j WHERE j.status='queued' AND j.available_at<=? AND NOT EXISTS \
     (SELECT 1 FROM jobs active WHERE active.dsp_id=j.dsp_id AND active.status IN ",
    job_statuses!(leased),
    ") ORDER BY (SELECT COALESCE(MAX(completed_at),'') FROM jobs previous \
     WHERE previous.dsp_id=j.dsp_id),j.created_at LIMIT 200"
);
const CANCEL: &str = concat!(
    "UPDATE jobs SET status='cancelled',message='Cancelled',completed_at=?,\
     lease_owner=NULL,lease_until=NULL WHERE status IN ",
    job_statuses!(active)
);
const START: &str = "UPDATE jobs SET status='running',attempt=attempt+1,started_at=?,lease_owner=?,\
    lease_until=?,message='Starting collection' WHERE id=?";
const PROGRESS: &str = concat!(
    "UPDATE jobs SET progress=?,message=?,status=? WHERE id=? AND lease_owner=? AND status IN ",
    job_statuses!(leased)
);
const FINISH: &str = "UPDATE jobs SET status=?,progress=?,message=?,error=?,completed_at=?,\
    available_at=?,lease_owner=NULL,lease_until=NULL WHERE id=?";
const CANCEL_RUNNING_METRICS: &str = "UPDATE job_metrics SET owner='',metrics=json_set(metrics,\
    '$.outcome','cancelled','$.error','job_cancelled','$.phase',NULL,'$.finishedAt',?) \
    WHERE json_extract(metrics,'$.outcome')='running' \
    AND job_id IN (SELECT id FROM jobs WHERE status='cancelled')";
const INTERRUPT_METRICS: &str = concat!(
    "UPDATE job_metrics SET owner='',metrics=json_set(metrics,'$.outcome','interrupted',\
     '$.error','worker_interrupted','$.phase',NULL,'$.finishedAt',?) \
     WHERE json_extract(metrics,'$.outcome')='running' \
     AND job_id IN (SELECT id FROM jobs WHERE status IN ",
    job_statuses!(leased),
    " AND (? OR lease_until<?))"
);
const RECOVER: &str = concat!(
    "UPDATE jobs SET status=CASE WHEN attempt>=max_attempts THEN 'failed' ELSE 'queued' END,\
     message='Recovered interrupted collection',error='worker_interrupted',available_at=?,\
     completed_at=CASE WHEN attempt>=max_attempts THEN ? ELSE NULL END,\
     lease_owner=NULL,lease_until=NULL WHERE status IN ",
    job_statuses!(leased),
    " AND (? OR lease_until<?)"
);

/// Which unfinished jobs a cancellation reaches.
pub enum CancelJobs<'a> {
    Job {
        id: &'a str,
        dsp: &'a str,
    },
    Provider {
        dsp: &'a str,
        provider: Provider,
    },
    /// Every job of one kind, when the page that owns it goes off.
    Kind {
        dsp: &'a str,
        kind: &'a str,
    },
    Dsp(&'a str),
}
/// What a worker needs to know about a provider's connection before it collects.
pub(crate) struct ConnectionLease {
    pub enabled: bool,
    pub revision: i64,
}
impl FromRow for ConnectionLease {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            enabled: row.get("enabled")?,
            revision: row.get("revision")?,
        })
    }
}
struct Claimable {
    id: String,
    dsp_id: String,
    kind: String,
}
impl FromRow for Claimable {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            dsp_id: row.get("dsp_id")?,
            kind: row.get("kind")?,
        })
    }
}
/// The parts of a job its outcome's log entry is written from.
pub struct JobFacts<'a> {
    pub idempotency_key: &'a str,
    pub provider: Option<Provider>,
    pub attempt: i64,
    pub max_attempts: i64,
    pub request: &'a str,
    pub started_at: Option<&'a str>,
}
impl<'a> From<&'a JobRow> for JobFacts<'a> {
    fn from(job: &'a JobRow) -> Self {
        Self {
            idempotency_key: &job.idempotency_key,
            provider: Some(job.provider()),
            attempt: job.attempt,
            max_attempts: job.max_attempts,
            request: &job.request,
            started_at: job.started_at.as_deref(),
        }
    }
}
impl Store {
    pub(crate) fn connection_lease(
        &self,
        dsp: &str,
        provider: Provider,
    ) -> Result<Option<ConnectionLease>> {
        self.collector(dsp, provider)?.one_as(
            "SELECT enabled,revision FROM connections WHERE provider=?",
            [provider.id()],
        )
    }
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
    /// Queues one request, answering with the job's public JSON, as collection requests do.
    pub fn enqueue_for(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        provider: Provider,
        request: &Value,
    ) -> Result<Value> {
        let job = self
            .enqueue_batch(id, actor, &[(key.into(), provider, request.clone())])?
            .remove(0);
        Ok(serde_json::to_value(job)?)
    }
    pub fn enqueue_batch(
        &self,
        id: &str,
        actor: Option<&str>,
        requests: &[(String, Provider, Value)],
    ) -> Result<Vec<PublicJob>> {
        self.ensure_dsp_active(id)?;
        let connections = requests
            .iter()
            .map(|(_, provider, _)| {
                let connection = self.connection_lease(id, *provider)?.unwrap();
                ensure(connection.enabled, "connection_required", 409)?;
                Ok(connection)
            })
            .collect::<Result<Vec<_>>>()?;
        let known = JobKind::known()?;
        let capacity = requests
            .iter()
            .map(|(_, provider, request)| {
                crate::manifest::registry()
                    .keepers()
                    .find(|keeper| keeper.keeps() == provider.collector().job_kind_for(request))
                    .map_or(5, |keeper| keeper.queue_capacity())
                    .clamp(5, 31)
            })
            .min()
            .unwrap_or(5);
        self.jobs.transaction(|| {
            let existing = requests
                .iter()
                .map(|(key, provider, request)| {
                    let row: Option<JobRow> = self.jobs.one_as(
                        "SELECT * FROM jobs WHERE dsp_id=? AND idempotency_key=?",
                        [id, key],
                    )?;
                    if let Some(row) = &row {
                        ensure(
                            row.kind.as_str() == provider.collector().job_kind_for(request)
                                && serde_json::from_str::<Value>(&row.request)? == *request,
                            "idempotency_conflict",
                            409,
                        )?;
                    }
                    Ok(row)
                })
                .collect::<Result<Vec<_>>>()?;
            // A manual sync owns the DSP until every job in its batch stops.
            // Check before inserting any job, in the same transaction, while
            // allowing retries of a request that already queued successfully.
            if actor.is_some() && existing.iter().any(Option::is_none) {
                ensure(
                    self.jobs.count(ACTIVE_COUNT, [id, &known])? == 0,
                    "sync_in_progress",
                    409,
                )?;
            }
            requests
                .iter()
                .zip(&connections)
                .zip(existing)
                .map(|(((key, provider, request), connection), existing)| {
                    if let Some(row) = existing {
                        return self.public_job(row);
                    }
                    ensure(
                        self.jobs.count(ACTIVE_COUNT, [id, &known])? < capacity,
                        "queue_full",
                        429,
                    )?;
                    let job = self.insert_job(
                        id,
                        actor,
                        key,
                        provider.collector().job_kind_for(request),
                        connection.revision,
                        request,
                    )?;
                    self.public_job(self.job_row(&job, None)?)
                })
                .collect()
        })
    }
    /// Writes one queued job, answering with its id. Every job is queued here. The table
    /// takes any kind, so a kind no registered collector collects is refused here instead,
    /// with the error the table's list of kinds refused it with.
    pub fn insert_job(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        kind: &str,
        revision: i64,
        request: &Value,
    ) -> Result<String> {
        ensure(
            Provider::from_job_kind(kind).is_ok(),
            "operation_failed",
            500,
        )?;
        let job = crypto::id("job")?;
        self.jobs.exec(
            INSERT,
            params![
                job,
                id,
                self.config.environment,
                kind,
                now(),
                iso(),
                self.config.release,
                actor,
                revision,
                key,
                serde_json::to_string(request)?
            ],
        )?;
        Ok(job)
    }
    /// Cancels the unfinished jobs the filter names; a finished job is left as it ended.
    pub fn cancel_jobs(&self, filter: CancelJobs<'_>) -> Result<usize> {
        let (at, kind);
        let (scope, values): (&str, Vec<&dyn rusqlite::ToSql>) = match &filter {
            CancelJobs::Job { id, dsp } => (" AND id=? AND dsp_id=?", vec![id, dsp]),
            CancelJobs::Provider { dsp, provider } => {
                kind = serde_json::to_string(&provider.job_kinds().collect::<Vec<_>>())?;
                (
                    " AND dsp_id=? AND kind IN (SELECT value FROM json_each(?))",
                    vec![dsp, &kind],
                )
            }
            CancelJobs::Kind { dsp, kind } => (" AND dsp_id=? AND kind=?", vec![dsp, kind]),
            CancelJobs::Dsp(dsp) => (" AND dsp_id=?", vec![dsp]),
        };
        at = iso();
        let mut bound: Vec<&dyn rusqlite::ToSql> = vec![&at];
        bound.extend(values);
        self.jobs
            .exec(&format!("{CANCEL}{scope}"), bound.as_slice())
    }
    pub fn cancel(&self, id: &str, dsp: &str) -> Result<PublicJob> {
        let row = self.job_row(id, Some(dsp))?;
        self.cancel_jobs(CancelJobs::Job { id, dsp })?;
        let provider = row.provider();
        provider.collector().discard(self, dsp, Some(id))?;
        self.clear_live(dsp, provider, Some(id))?;
        self.public_job(self.job_row(id, Some(dsp))?)
    }
    pub fn cancel_provider(&self, id: &str, provider: Provider) -> Result<()> {
        self.clear_live(id, provider, None)?;
        self.cancel_jobs(CancelJobs::Provider { dsp: id, provider })?;
        provider.collector().discard(self, id, None)
    }
    pub fn cancel_dsp(&self, id: &str) -> Result<()> {
        for provider in Provider::all() {
            self.clear_live(id, provider, None)?;
        }
        for provider in Provider::all() {
            provider.collector().discard(self, id, None)?;
        }
        self.cancel_jobs(CancelJobs::Dsp(id))?;
        Ok(())
    }
    /// The job is still this worker's to run. Answers with its DSP.
    pub fn guard(&self, id: &str, owner: &str) -> Result<Dsp> {
        let row = self.job_row(id, None)?;
        ensure(
            row.held_by(owner) && row.lease_until.unwrap_or(0) > now(),
            "job_cancelled",
            409,
        )?;
        let dsp = self.ensure_dsp_active(&row.dsp_id)?;
        ensure(
            self.feature_enabled(
                &row.dsp_id,
                crate::tenancy::catalog::automation(row.kind.as_str()),
            )?,
            "feature_disabled",
            409,
        )?;
        if let Some(actor) = row.actor_id.as_deref() {
            let user: (UserStatus, bool) = self
                .platform
                .one_as(
                    "SELECT status,platform_owner FROM users WHERE id=?",
                    [actor],
                )?
                .ok_or_else(|| Error::new("permission_denied", 403))?;
            let (status, platform_owner) = user;
            ensure(
                status == UserStatus::Active
                    && (platform_owner
                        || self.grant(actor, &row.dsp_id)?.is_some_and(|grant| {
                            grant.owner
                                || grant.permissions.iter().any(|p| {
                                    p == &crate::tenancy::catalog::collection_permission(
                                        row.kind.as_str(),
                                    )
                                })
                        })),
                "permission_denied",
                403,
            )?;
        }
        let connection = self
            .connection_lease(&row.dsp_id, row.provider())?
            .ok_or_else(|| Error::new("connection_required", 409))?;
        ensure(
            connection.enabled && connection.revision == row.connection_revision,
            "connection_changed",
            409,
        )?;
        Ok(dsp)
    }
    pub fn recover_jobs(&self, all: bool) -> Result<()> {
        self.jobs.transaction(|| {
            if all {
                self.jobs.exec(CANCEL_RUNNING_METRICS, [iso()])?;
            }
            self.jobs
                .exec(INTERRUPT_METRICS, params![iso(), all, now()])?;
            self.jobs.exec(RECOVER, params![now(), iso(), all, now()])?;
            Ok(())
        })
    }
    /// The next job a worker may run, now leased to `owner`.
    pub fn claim_job(
        &self,
        owner: &str,
        eligible: impl Fn(&str, Provider) -> bool,
    ) -> Result<Option<JobRow>> {
        self.jobs.transaction(|| {
            if self.jobs.count(LEASED_COUNT, [])? >= self.config.browser_capacity as i64 {
                return Ok(None);
            }
            // A kind no registered provider runs is not a job this release can claim.
            let queued: Vec<Claimable> = self.jobs.query_as(CLAIMABLE, [now()])?;
            let Some(Claimable { id, .. }) = queued.into_iter().find(|job| {
                Provider::from_job_kind(&job.kind).is_ok_and(|(p, _)| eligible(&job.dsp_id, p))
            }) else {
                return Ok(None);
            };
            self.jobs
                .exec(START, params![iso(), owner, now() + 120000, id])?;
            let job = self.job_row(&id, None)?;
            self.jobs.exec(
                "INSERT INTO job_metrics(job_id,attempt,owner,metrics) VALUES (?,?,?,?)",
                params![
                    job.id,
                    job.attempt,
                    owner,
                    serde_json::to_string(&Metrics::start(&job))?
                ],
            )?;
            Ok(Some(job))
        })
    }
    pub fn progress(
        &self,
        id: &str,
        owner: &str,
        progress: i64,
        message: &str,
        status: ActiveJobStatus,
    ) -> Result<()> {
        let count = self.jobs.exec(
            PROGRESS,
            params![progress.clamp(0, 99), message, status.as_str(), id, owner],
        )?;
        ensure(count == 1, "job_cancelled", 409)
    }
    pub fn finish(&self, id: &str, owner: &str, error: Option<&str>) -> Result<()> {
        let row = self.job_row(id, None)?;
        if !row.held_by(owner) {
            return Ok(());
        }
        let retry = error.is_some_and(|e| Code::retryable().any(|code| code.as_str() == e))
            && row.attempt < row.max_attempts;
        let (status, message) = if retry {
            (JobStatus::Queued, "Retry scheduled")
        } else if error.is_some() {
            (JobStatus::Failed, "Collection could not finish")
        } else {
            (JobStatus::Succeeded, "Collection completed")
        };
        self.jobs.exec(
            FINISH,
            params![
                status,
                if error.is_some() { row.progress } else { 100 },
                message,
                error,
                if retry { None } else { Some(iso()) },
                now() + retry_delay(id, row.attempt),
                id
            ],
        )?;
        let provider = row.provider();
        self.clear_live(&row.dsp_id, provider, Some(id))?;
        if !retry {
            provider.collector().discard(self, &row.dsp_id, Some(id))?;
        }
        Ok(())
    }
    // What an outcome's log entry says beyond pass or fail: the schedule that
    // queued it, and the provider, collected date and run time.
    pub fn job_facts(&self, dsp: &str, job: &JobFacts<'_>) -> (Option<String>, Vec<AuditChange>) {
        let schedule = job
            .idempotency_key
            .strip_prefix("schedule:")
            .and_then(|key| key.split(':').next())
            .and_then(|id| self.collection_schedule(dsp, id).ok())
            .map(|row| row.name);
        // Only a registered kind is ever claimed, so a job always names its provider.
        let provider = job.provider.map_or("", Provider::id);
        let mut facts = vec![("provider", None, Some(provider.to_owned()))];
        if job.attempt > 1 || job.max_attempts > 1 {
            facts.push((
                "attempt",
                None,
                Some(format!("{} of {}", job.attempt, job.max_attempts)),
            ));
        }
        let request = serde_json::from_str::<Value>(job.request).unwrap_or_default();
        if let Some(date) = request["date"].as_str() {
            facts.push(("date", None, Some(date.to_owned())));
        }
        if let Some(started) = job
            .started_at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        {
            let seconds = (now() - started.timestamp_millis()).max(0) / 1000;
            facts.push(("duration", None, Some(seconds.to_string())));
        }
        (schedule, facts)
    }
}
// Stable per-job jitter survives restarts and disperses DSP retries. No secret
// material or provider identity participates in the delay.
fn retry_delay(id: &str, attempt: i64) -> i64 {
    use sha2::{Digest, Sha256};
    let base = 30000 * 2_i64.pow(attempt.clamp(0, 8) as u32);
    let digest = Sha256::digest(format!("{id}:{attempt}"));
    base + i64::from(u32::from_le_bytes(digest[..4].try_into().unwrap())) % (base / 2 + 1)
}

#[cfg(test)]
#[path = "../../tests/backend/jobs/queue/retry_tests.rs"]
mod retry_tests;
