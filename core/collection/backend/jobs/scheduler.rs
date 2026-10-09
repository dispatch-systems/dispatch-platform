use super::executor::execute;
use crate::{
    Error, Result, State,
    collection::{
        api::jobs::{JobKind, JobRow},
        registry::Provider,
    },
    db::{FromRow, Row, now},
    foundation::crypto,
    job_statuses,
    manifest::registry,
    mcp::activity,
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
fn failed(event: &str, error: &Error) {
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
    // A year's retention does not need checking every minute.
    audit_pruned: i64,
    // When each feature's upkeep was last due, in the registry's order.
    upkept: Vec<i64>,
    // When agents' calls were last written down for the Activity log.
    activity_written: i64,
}
impl Scheduler {
    async fn cleanup(&mut self) {
        let prune_audit = now() - self.audit_pruned >= 24 * 60 * 60 * 1000;
        if prune_audit {
            self.audit_pruned = now();
        }
        let agents_used = self.state.agents.take();
        let agent_keys: Vec<String> = agents_used.iter().map(|(key, _)| key.clone()).collect();
        let cache_state = self.state.clone();
        let result = self
            .state
            .run_bookkeeping(move |db| {
                // Expired invitations may change an owner shown in session listings.
                // Token/checkpoint/audit cleanup does not change provider-derived data.
                cache_state.read_cache.invalidate_listings();
                if prune_audit {
                    db.prune_audit()?;
                }
                if !agents_used.is_empty() {
                    db.record_agent_use(&agents_used)?;
                }
                // Expired access tokens have no remaining authentication purpose, in the
                // platform's directory or any DSP's.
                const EXPIRED: [&str; 2] = [
                    "DELETE FROM sessions WHERE expires_at<?",
                    "DELETE FROM resets WHERE expires_at<?",
                ];
                // An accepted invitation stays 90 days, so Diagnostics can show that it was
                // accepted. It can no longer be used. Invitations are each DSP's own: the ones
                // the platform's directory still holds stay as the release before left them.
                const INVITATIONS: &str = "DELETE FROM invitations WHERE expires_at<?1 AND \
                     (used_at IS NULL OR used_at<?1-7776000000)";
                db.platform.transaction(|| {
                    for sql in EXPIRED
                        .iter()
                        .chain(&["DELETE FROM throttle WHERE reset_at<?"])
                    {
                        db.platform.exec(sql, [now()])?;
                    }
                    Ok(())
                })?;
                db.prune_oauth()?;
                let dsps: Vec<(String,)> = db.platform.query_as(
                    "SELECT id FROM dsps WHERE status IN ('active','suspended')",
                    [],
                )?;
                for (dsp,) in dsps {
                    let people = db.dsp(&dsp)?;
                    people.transaction(|| {
                        for sql in EXPIRED.iter().chain(&[INVITATIONS]) {
                            people.exec(sql, [now()])?;
                        }
                        Ok(())
                    })?;
                    drop(people);
                    for provider in Provider::all() {
                        provider.collector().prune(db, &dsp)?;
                    }
                }
                Ok(())
            })
            .await;
        if let Err(error) = result {
            // Nothing was committed, so the keys' last use is written next time.
            self.state.agents.unsaved(&agent_keys);
            failed("checkpoint_cleanup_failed", &error);
        }
        // Agents' calls past 90 days, a step a minute, apart from the rest so a long
        // backlog never holds the lock for the other cleanup.
        if let Err(error) = self
            .state
            .run_bookkeeping(|db| db.prune_agent_activity())
            .await
        {
            failed("agent_activity_prune_failed", &error);
        }
        self.maintain().await;
    }
    /// Writes down agents' calls for the Activity log every five seconds, or sooner once a
    /// batch is waiting: one write for many calls, never one per call.
    async fn write_activity(&mut self) {
        let waiting = self.state.activity.pending();
        if waiting == 0
            || (waiting < activity::BATCH && now() - self.activity_written < activity::EVERY_MS)
        {
            return;
        }
        self.activity_written = now();
        if let Err(error) = activity::flush(&self.state).await {
            failed("agent_activity_failed", &error);
        }
    }
    /// Each feature's upkeep, in the registry's order, told whether it is due.
    async fn maintain(&mut self) {
        let tasks = registry().features.iter().flat_map(|f| f.maintenance);
        for (task, last) in tasks.zip(self.upkept.iter_mut()) {
            let due = now() - *last >= task.every.as_millis() as i64;
            if due {
                *last = now();
            }
            (task.run)(self.state.clone(), due).await;
        }
    }
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
        audit_pruned: 0,
        upkept: vec![
            0;
            registry()
                .features
                .iter()
                .map(|f| f.maintenance.len())
                .sum()
        ],
        activity_written: 0,
    };
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut checkpoint_cleanup = tokio::time::interval(Duration::from_secs(60));
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
            _ = checkpoint_cleanup.tick() => scheduler.cleanup().await,
            _ = timer.tick() => {
                scheduler.write_activity().await;
                scheduler.tick().await;
            },
        }
    }
    // What agents called since the last write is not lost to a restart.
    if let Err(error) = activity::flush(&scheduler.state).await {
        failed("agent_activity_failed", &error);
    }
    scheduler.state.browsers.close().await;
    while scheduler.tasks.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/jobs/scheduler/refresh_tests.rs"]
mod tests;
