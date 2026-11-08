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
    manifest::{MAX_QUEUE_CAPACITY, MIN_QUEUE_CAPACITY},
};
use rusqlite::params;
use serde_json::Value;

// Each query that reads, counts or runs a job takes `JobKind::known()`, so that none
// reaches a job of a kind this release doesn't run.
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
const CLAIM_PAGE_SIZE: i64 = 200;
const CLAIMABLE: &str = concat!(
    "SELECT j.id,j.dsp_id,j.kind FROM jobs j WHERE j.status='queued' AND j.available_at<=? \
     AND j.kind IN (SELECT value FROM json_each(?)) AND NOT EXISTS \
     (SELECT 1 FROM jobs active WHERE active.dsp_id=j.dsp_id AND active.status IN ",
    job_statuses!(leased),
    ") ORDER BY (SELECT COALESCE(MAX(completed_at),'') FROM jobs previous \
     WHERE previous.dsp_id=j.dsp_id),j.created_at,j.id LIMIT ? OFFSET ?"
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
    /// The collection it runs, as its job kind.
    pub kind: &'a str,
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
            kind: job.kind.as_str(),
            attempt: job.attempt,
            max_attempts: job.max_attempts,
            request: &job.request,
            started_at: job.started_at.as_deref(),
        }
    }
}
impl Store {
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
                    .map_or(MIN_QUEUE_CAPACITY, |keeper| keeper.queue_capacity())
                    .clamp(MIN_QUEUE_CAPACITY, MAX_QUEUE_CAPACITY)
            })
            .min()
            .unwrap_or(MIN_QUEUE_CAPACITY);
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
    /// Deletes the jobs of a DSP being deleted, with their metrics, and what its live readers
    /// recorded.
    pub fn forget_dsp_jobs(&self, id: &str) -> Result<()> {
        self.jobs.transaction(|| {
            self.jobs.exec("DELETE FROM jobs WHERE dsp_id=?", [id])?;
            self.forget_reads(id)
        })
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
        let page = crate::tenancy::catalog::automation(row.kind.as_str());
        ensure(
            match page {
                Some(page) => self.feature_enabled(&row.dsp_id, page)?,
                None => false,
            },
            "feature_disabled",
            409,
        )?;
        if let Some(actor) = row.actor_id.as_deref() {
            // Whoever asked: a platform owner, or one of the DSP's own people.
            let owner: Option<(UserStatus, bool)> = self.platform.one_as(
                "SELECT status,platform_owner FROM users WHERE id=? AND platform_owner=1",
                [actor],
            )?;
            let user: (UserStatus, bool) = match owner {
                Some(owner) => owner,
                None => self
                    .dsp(&row.dsp_id)?
                    .one_as("SELECT status,0 FROM users WHERE id=?", [actor])?
                    .ok_or_else(|| Error::new("permission_denied", 403))?,
            };
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
            let (at, known) = (now(), JobKind::known()?);
            let mut offset = 0;
            let id = loop {
                let queued: Vec<Claimable> = self
                    .jobs
                    .query_as(CLAIMABLE, params![at, known, CLAIM_PAGE_SIZE, offset])?;
                let count = queued.len() as i64;
                if let Some(Claimable { id, .. }) = queued.into_iter().find(|job| {
                    Provider::from_job_kind(&job.kind).is_ok_and(|(p, _)| eligible(&job.dsp_id, p))
                }) {
                    break id;
                }
                if count < CLAIM_PAGE_SIZE {
                    return Ok(None);
                }
                // The transaction keeps the ordering stable while ineligible pages are skipped.
                offset += count;
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
            .and_then(|id| self.schedule_label(dsp, id).ok().flatten())
            .map(str::to_owned);
        // Only a registered kind is ever claimed, so a job always names its provider.
        let provider = job.provider.map_or("", Provider::id);
        // The collection names the outcome; the provider words one written before it did.
        let mut facts = vec![
            ("provider", None, Some(provider.to_owned())),
            ("collection", None, Some(job.kind.to_owned())),
        ];
        if job.attempt > 1 || job.max_attempts > 1 {
            facts.push((
                "attempt",
                None,
                Some(format!("{} of {}", job.attempt, job.max_attempts)),
            ));
        }
        let request = serde_json::from_str::<Value>(job.request).unwrap_or_default();
        // The day a job read, or the first and last of the days it read.
        let dates: Vec<&str> = request["date"]
            .as_str()
            .into_iter()
            .chain(
                request["dates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .collect();
        if let (Some(first), Some(last)) = (dates.first(), dates.last()) {
            facts.push(("date", None, Some((*first).to_owned())));
            if last != first {
                facts.push(("through", None, Some((*last).to_owned())));
            }
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
