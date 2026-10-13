use super::executor::execute;
use crate::{
    Error, Result, State,
    collection::api::jobs::{JobKind, JobRow},
    db::{FromRow, Row, now},
    foundation::crypto,
    job_statuses,
    manifest::registry,
    server::cache::DataDomain,
};
use rusqlite::params;
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

// Poll indexed queue/lease state without a write lock. A queued job counts only if this
// release runs its kind, `?2` (`JobKind::known()`).
const READY: &str = concat!(
    "SELECT EXISTS(SELECT 1 FROM jobs WHERE status='queued' AND available_at<=?1 \
     AND kind IN (SELECT value FROM json_each(?2))) queued,\
     EXISTS(SELECT 1 FROM jobs WHERE status IN ",
    job_statuses!(leased),
    " AND lease_until<?1) expired"
);
const WAITING_MESSAGE: &str = "UPDATE jobs SET message=?1 WHERE status='queued' \
    AND available_at<=?2 AND message<>?1 AND kind IN (SELECT value FROM json_each(?3))";

struct Ready {
    queued: bool,
    expired: bool,
}
impl FromRow for Ready {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            queued: row.get("queued")?,
            expired: row.get("expired")?,
        })
    }
}
pub(super) fn failed(event: &str, error: &Error) {
    crate::foundation::observability::event("error", event, json!({"error":error.code}));
}

/// When the schedules' deadlines are read: at once when a schedule changes, else every
/// minute. A failed read waits five seconds before the next try, as a failed schedule does,
/// so a failure that lasts is logged every few seconds rather than every tick; the deadlines
/// read before keep running meanwhile.
struct Refresh {
    revision: u64,
    read_at: i64,
    retry_at: i64,
}
impl Refresh {
    const EVERY_MS: i64 = 60_000;
    const RETRY_MS: i64 = 5_000;
    fn new() -> Self {
        Self {
            revision: u64::MAX,
            read_at: 0,
            retry_at: 0,
        }
    }
    fn due(&self, revision: u64, now: i64) -> bool {
        now >= self.retry_at && (revision != self.revision || now - self.read_at >= Self::EVERY_MS)
    }
    fn read(&mut self, revision: u64, now: i64) {
        self.revision = revision;
        self.read_at = now;
    }
    fn failed(&mut self, now: i64) {
        self.retry_at = now + Self::RETRY_MS;
    }
}

struct Scheduler {
    state: Arc<State>,
    owner: String,
    tasks: tokio::task::JoinSet<String>,
    running_dsps: HashSet<String>,
    deadlines: HashMap<String, i64>,
    refresh: Refresh,
}
impl Scheduler {
    async fn run_due_schedules(&mut self) {
        let state = &self.state;
        let revision = state
            .schedule_revision
            .load(std::sync::atomic::Ordering::Acquire);
        if self.refresh.due(revision, now()) {
            match state.read(|db| db.schedule_deadlines()).await {
                Ok(values) => {
                    self.deadlines = values.into_iter().collect();
                    self.refresh.read(revision, now());
                }
                Err(error) => {
                    self.refresh.failed(now());
                    failed("scheduler_refresh_failed", &error);
                }
            }
        }
        let due: Vec<_> = self
            .deadlines
            .iter()
            .filter(|(_, at)| **at <= now())
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            let dsp = id.clone();
            match state
                .run_scoped(id.clone(), DataDomain::SCHEDULES, move |db| {
                    db.schedule_due(&dsp)
                })
                .await
            {
                Ok(Some(next)) => {
                    self.deadlines.insert(id, next);
                }
                Ok(None) => {
                    self.deadlines.remove(&id);
                }
                Err(error) => {
                    self.deadlines.insert(id, now() + 5000);
                    failed("scheduler_tick_failed", &error);
                }
            }
        }
    }
    async fn claim(&self) -> Result<Option<JobRow>> {
        let pool = self.state.clone();
        let owner = self.owner.clone();
        let running = self.running_dsps.clone();
        self.state
            .run_bookkeeping(move |db| {
                let memory_ready = (pool.config.fixture && pool.config.fixture_url.is_none())
                    || pool.browsers.admission().can_start;
                let message = if memory_ready {
                    "Waiting for a browser"
                } else {
                    "Waiting for available memory"
                };
                db.jobs
                    .exec(WAITING_MESSAGE, params![message, now(), JobKind::known()?])?;
                db.claim_job(&owner, |id, provider| {
                    !running.contains(id)
                        && pool
                            .browsers
                            .get_for(id, provider)
                            .map(|s| !s.busy() && !s.closed())
                            .unwrap_or_else(|| {
                                memory_ready
                                    && pool.browsers.active() < pool.config.browser_capacity
                            })
                })
            })
            .await
    }
    async fn tick(&mut self) {
        self.state.expire_browsers().await;
        self.run_due_schedules().await;
        // Recovery still runs on the first tick after expiry, including quiet DSPs.
        let ready = match self
            .state
            .read(|db| {
                db.jobs
                    .one_as::<Ready>(READY, params![now(), JobKind::known()?])
            })
            .await
        {
            Ok(Some(value)) => value,
            Ok(None) => return,
            Err(error) => return failed("job_poll_failed", &error),
        };
        if ready.expired
            && let Err(error) = self
                .state
                .run_bookkeeping(|db| db.recover_jobs(false))
                .await
        {
            failed("job_recovery_failed", &error);
        }
        if !ready.queued && !ready.expired {
            return;
        }
        while self.tasks.len() < self.state.config.browser_capacity {
            match self.claim().await {
                Ok(Some(job)) => {
                    let state = self.state.clone();
                    let owner = self.owner.clone();
                    let dsp = job.dsp_id.clone();
                    self.running_dsps.insert(dsp.clone());
                    self.tasks.spawn(async move {
                        execute(state, job, owner).await;
                        dsp
                    });
                }
                Ok(None) => break,
                Err(error) => {
                    failed("job_claim_failed", &error);
                    break;
                }
            }
        }
    }
}
pub async fn start(state: Arc<State>, mut stop: tokio::sync::watch::Receiver<bool>) -> Result<()> {
    let mut scheduler = Scheduler {
        state,
        owner: crypto::id("worker")?,
        tasks: tokio::task::JoinSet::new(),
        running_dsps: HashSet::new(),
        deadlines: HashMap::new(),
        refresh: Refresh::new(),
    };
    let mut upkeep = super::maintenance::Maintenance::new();
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut checkpoint_cleanup = tokio::time::interval(Duration::from_secs(60));
    checkpoint_cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = crate::cancelled(&mut stop) => break,
            result = scheduler.tasks.join_next(), if !scheduler.tasks.is_empty() => {
                match result {
                    Some(Ok(dsp)) => {
                        scheduler.running_dsps.remove(&dsp);
                    }
                    Some(Err(_)) => return Err(Error::new("collector_task_failed", 500)),
                    None => {}
                }
            },
            result = upkeep.tasks.join_next(), if !upkeep.tasks.is_empty() => upkeep.finished(result)?,
            _ = checkpoint_cleanup.tick() => upkeep.start(&scheduler.state),
            _ = timer.tick() => {
                if let Some(agents) = registry().agents {
                    (agents.tick)(scheduler.state.clone(), false).await;
                }
                scheduler.tick().await;
            },
        }
    }
    // Cancel external upkeep before closing browser resources. Database closures already
    // running finish under their normal lock; the next pass safely resumes maintenance.
    upkeep.tasks.shutdown().await;
    // What the agents' piece holds in memory is not lost to a restart.
    if let Some(agents) = registry().agents {
        (agents.tick)(scheduler.state.clone(), true).await;
    }
    scheduler.state.browsers.close().await;
    while scheduler.tasks.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/jobs/scheduler/refresh_tests.rs"]
mod tests;
