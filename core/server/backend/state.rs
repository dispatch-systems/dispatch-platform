//! What the server shares across requests and background tasks.
use crate::{
    Error, Result,
    collection::browser,
    db,
    foundation::{config, observability},
    manifest,
    server::{cache, http, live, mail, presence, startup},
};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Semaphore;

tokio::task_local! {
    /// Set while a claimed job runs: see `State::patiently`.
    static PATIENT: ();
}
/// How long a running job's database call waits for its turn: past the longest write it may
/// queue behind, such as another DSP's publication, where a request gives up after two seconds.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);
/// How many of the database's slots work outside a request holds at once. The rest are left to
/// requests, so every DSP's upkeep starting together keeps no page waiting behind it.
const BACKGROUND_SLOTS: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Write,
    Unpublished,
}

enum CacheChange {
    All,
    Bookkeeping,
    Tenant(String, cache::DataDomain),
}

pub struct State {
    pub config: config::Config,
    pub key: Vec<u8>,
    pub assets: std::collections::HashMap<String, http::Asset>,
    pub db_slots: Arc<Semaphore>,
    /// The turns at `db_slots` work outside a request takes: `BACKGROUND_SLOTS` at once.
    pub background_slots: Arc<Semaphore>,
    pub db_queue: Arc<Semaphore>,
    // Serializes short state transitions across the platform, jobs and tenant databases.
    pub transition: RwLock<()>,
    pub pool: Mutex<Vec<db::Store>>,
    pub read_cache: cache::ReadCache,
    pub read_turns: cache::ReadTurns,
    // Unpublished writers serialize by DSP before taking any database worker.
    unpublished_turns: cache::ReadTurns,
    pub data_revision: std::sync::atomic::AtomicU64,
    pub schedule_revision: std::sync::atomic::AtomicU64,
    pub password_slots: Arc<Semaphore>,
    pub mail_transport: Mutex<mail::TransportHealth>,
    // Wakes the mailer when a request queues mail, instead of it waiting for its next tick.
    pub mail_wake: tokio::sync::Notify,
    // Wakes the live readers when a write switches a feature or saves settings, so each DSP's
    // reading follows at once rather than at the next minute.
    pub readers_wake: tokio::sync::Notify,
    pub browsers: browser::Manager,
    pub updates: live::Updates,
    /// Every feature's own live channels.
    pub topics: live::Topics,
    pub presence: presence::Presence,
    /// What the agents' piece keeps in memory for as long as the server runs, in its own
    /// terms; none in a build without it.
    pub agents: Option<Box<dyn std::any::Any + Send + Sync>>,
    /// The object store the DSPs' pictures are kept in, when the server has one.
    pub objects: Option<db::objects::Objects>,
    /// Where the DSPs' backups are kept, when the server has somewhere.
    pub backups: Option<crate::backups::Vault>,
    // Wakes what takes backups when one is asked for by hand, rather than at its next minute.
    pub backups_wake: tokio::sync::Notify,
}
impl State {
    /// Reads the registry, which the app installs before it builds one.
    pub fn new(config: config::Config) -> Result<Arc<Self>> {
        Self::initialize(config, browser::Manager::default())
    }
    pub(crate) fn for_collection_verification(
        config: config::Config,
        authority: browser::browseros::VerificationAuthority,
    ) -> Result<Arc<Self>> {
        let runtime = authority.runtime(
            &config.environment_root().join("browser-runs"),
            config.browser_capacity,
        )?;
        Self::initialize(config, browser::Manager::with_verified_runtime(runtime))
    }
    fn initialize(config: config::Config, browsers: browser::Manager) -> Result<Arc<Self>> {
        let store = startup::prepare_server_store(&config)?;
        let agents = match manifest::registry().agents {
            Some(agents) => Some((agents.state)(&store)?),
            None => None,
        };
        let objects = config
            .objects
            .as_ref()
            .map(db::objects::Objects::new)
            .transpose()?;
        let backups = config
            .backups
            .as_ref()
            .map(crate::backups::Vault::new)
            .transpose()?;
        Ok(Arc::new(Self {
            key: store.key.clone(),
            assets: http::assets(&config.dashboard, &config.release)?,
            config,
            db_slots: Arc::new(Semaphore::new(4)),
            background_slots: Arc::new(Semaphore::new(BACKGROUND_SLOTS)),
            db_queue: Arc::new(Semaphore::new(64)),
            transition: RwLock::new(()),
            pool: Mutex::new(vec![store]),
            read_cache: cache::ReadCache::default(),
            read_turns: cache::ReadTurns::default(),
            unpublished_turns: cache::ReadTurns::default(),
            data_revision: std::sync::atomic::AtomicU64::new(0),
            schedule_revision: std::sync::atomic::AtomicU64::new(0),
            password_slots: Arc::new(Semaphore::new(2)),
            mail_transport: Mutex::new(mail::TransportHealth::default()),
            mail_wake: tokio::sync::Notify::new(),
            readers_wake: tokio::sync::Notify::new(),
            browsers,
            updates: live::Updates::new()?,
            topics: live::Topics::default(),
            presence: presence::Presence::default(),
            agents,
            objects,
            backups,
            backups_wake: tokio::sync::Notify::new(),
        }))
    }
    #[track_caller]
    pub fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        self.database(Access::Write, CacheChange::All, f)
    }
    /// Writes only reviewed bookkeeping: metrics, leases, outbox or invisible cleanup.
    /// Authorization still reads the database on every request; this skips derived-data
    /// invalidation, never the exclusive transition lock.
    #[track_caller]
    pub fn run_bookkeeping<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        self.database(Access::Write, CacheChange::Bookkeeping, f)
    }
    /// Writes only unpublished rows that foreground readers ignore. Each caller must recheck
    /// its ownership and feature/connection authority in `f`; final publication still uses an
    /// exclusive write. The shared transition protects tenant lifetime while WAL readers proceed.
    #[track_caller]
    pub fn run_unpublished<T: Send + 'static>(
        self: &Arc<Self>,
        dsp: impl Into<String>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        let caller = std::panic::Location::caller();
        let dsp = dsp.into();
        async move {
            let waiting = std::time::Instant::now();
            let turn = self
                .unpublished_turns
                .enter_for(dsp.clone(), PATIENCE)
                .await?;
            let wait_ms = waiting.elapsed().as_secs_f64() * 1000.0;
            if wait_ms >= 25.0 {
                observability::event(
                    "info",
                    "database.unpublished_wait",
                    serde_json::json!({"dspId":dsp,"waitMs":wait_ms,
                        "work":format!("{}:{}",caller.file(),caller.line())}),
                );
            }
            self.turn(
                Access::Unpublished,
                CacheChange::Bookkeeping,
                move |db| {
                    // Once begun, the worker retains this turn even if its caller leaves.
                    let _turn = turn;
                    db.ensure_dsp_active(&dsp)?;
                    f(db)
                },
                caller,
            )
            .await
        }
    }
    /// A write that says what it changes with `changed`, under the exclusive lock, once it
    /// knows and before it changes anything: a write route, once its caller is authorized.
    #[track_caller]
    pub(crate) fn run_declaring<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        self.database(Access::Write, CacheChange::Bookkeeping, f)
    }
    /// What a `run_declaring` write changes: one DSP's data, which evicts that DSP's cached
    /// reads and those every DSP shares, or with none, anything, which advances the global
    /// revision and so evicts every cached read.
    pub fn changed(&self, dsp: Option<&str>) {
        match dsp {
            Some(dsp) => self.read_cache.invalidate_dsp(dsp),
            None => {
                self.data_revision
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    /// What a `run_declaring` write that changes only `domain` changes: one DSP's reads that
    /// depend on it, and those every DSP shares that do, or with no DSP, anything, as `changed`.
    pub fn changed_in(&self, dsp: Option<&str>, domain: cache::DataDomain) {
        match dsp {
            Some(dsp) => self.read_cache.invalidate_tenant(dsp, domain),
            None => self.changed(None),
        }
    }
    /// Lets go of what the server's caches and stores hold of a deleted DSP: its cached reads,
    /// and the connections each idle store keeps to its databases. A store at work keeps its
    /// own until it's least recently used, as it can't open them again.
    pub fn forget_dsp(&self, dsp: &str) {
        self.read_cache.invalidate_dsp(dsp);
        let pool = self
            .pool
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        for store in pool.iter() {
            store.forget_dsp(dsp);
        }
    }
    /// A tenant mutation with known dependencies. Unknown writes must keep using `run`.
    #[track_caller]
    pub fn run_scoped<T: Send + 'static>(
        self: &Arc<Self>,
        dsp: impl Into<String>,
        domain: cache::DataDomain,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        self.database(Access::Write, CacheChange::Tenant(dsp.into(), domain), f)
    }
    /// Prepares a reviewed scoped write read-only, declares its impact, then writes in the
    /// same exclusive transition. `Unchanged` preserves only this DSP's PEOPLE scope; every
    /// other dependent read is invalidated as by `run_scoped`, before the writer, even on error.
    ///
    /// Like `run_bookkeeping`, this trusts its caller: Store has interior mutation. Preparation
    /// must perform only guards, reads and in-memory work, never writes, cache fills, events or
    /// outside work. It must prove all PEOPLE inputs unchanged by the actual prepared writer,
    /// or return `MayChange`. Extra proof failures must fall back, not introduce write refusals.
    /// Existing validation errors still propagate. Both callbacks must use the bound DSP.
    #[track_caller]
    pub fn run_scoped_prepared<P: Send + 'static, T: Send + 'static>(
        self: &Arc<Self>,
        dsp: impl Into<String>,
        domain: cache::DataDomain,
        prepare: impl FnOnce(&db::Store, &str) -> Result<(P, cache::PeopleImpact)> + Send + 'static,
        write: impl FnOnce(&db::Store, &str, P) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        let dsp = dsp.into();
        let state = self.clone();
        self.database(Access::Write, CacheChange::Bookkeeping, move |db| {
            let (prepared, people) = prepare(db, &dsp)?;
            state.read_cache.invalidate_prepared(&dsp, domain, people);
            write(db, &dsp, prepared)
        })
    }
    /// A write in the DSP's data by work acting for it, as upkeep, as `run_scoped`: made only
    /// while this backend serves the DSP, checked in the same turn, so work begun before the DSP
    /// was disabled changes nothing of it after (`dsp_unavailable`).
    #[track_caller]
    pub fn run_serving<T: Send + 'static>(
        self: &Arc<Self>,
        dsp: impl Into<String>,
        domain: cache::DataDomain,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        let dsp = dsp.into();
        let at = dsp.clone();
        self.run_scoped(dsp, domain, move |db| {
            db.ensure_dsp_active(&at)?;
            f(db)
        })
    }
    /// Runs a request's work that has no one to try it again, as a save. Its database calls wait
    /// their turn for up to a minute, where a request's are turned away with `platform_busy`
    /// after two seconds. Work outside a request, as a claimed job's, always waits so.
    pub async fn patiently<F: std::future::Future>(work: F) -> F::Output {
        PATIENT.scope((), work).await
    }
    #[track_caller]
    pub fn read<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        self.database(Access::Read, CacheChange::Bookkeeping, f)
    }
    /// The database work `f`, named in the log by the line that asked for it.
    #[track_caller]
    fn database<T: Send + 'static>(
        self: &Arc<Self>,
        access: Access,
        change: CacheChange,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> impl Future<Output = Result<T>> + Send {
        let caller = std::panic::Location::caller();
        self.turn(access, change, f, caller)
    }
    async fn turn<T: Send + 'static>(
        self: &Arc<Self>,
        access: Access,
        change: CacheChange,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
        caller: &'static std::panic::Location<'static>,
    ) -> Result<T> {
        // Bound both queued requests and blocking threads. Short bursts wait without
        // allocating another database pool or failing otherwise healthy requests.
        let started = std::time::Instant::now();
        let trace = observability::REQUEST_TRACE.try_with(Clone::clone).ok();
        let unavailable = |_| Error::new("platform_unavailable", 503);
        // Work outside a request has no one to try it again, so it waits as a job's does, and
        // takes one of the background turns first, so it never holds every slot.
        let background = trace.is_none();
        let (turn, queued, permit) = if background || PATIENT.try_with(|_| ()).is_ok() {
            tokio::time::timeout(PATIENCE, async {
                let turn = match background {
                    true => Some(self.background_slots.clone().acquire_owned().await?),
                    false => None,
                };
                let queued = self.db_queue.clone().acquire_owned().await?;
                Ok((turn, queued, self.db_slots.clone().acquire_owned().await?))
            })
            .await
            .map_err(|_| Error::busy("db_patient_timeout"))?
            .map_err(unavailable)?
        } else {
            let queued =
                self.db_queue
                    .clone()
                    .try_acquire_owned()
                    .map_err(|error| match error {
                        tokio::sync::TryAcquireError::NoPermits => Error::busy("db_queue_full"),
                        tokio::sync::TryAcquireError::Closed => Error::busy("db_queue_closed"),
                    })?;
            let permit = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                self.db_slots.clone().acquire_owned(),
            )
            .await
            .map_err(|_| Error::busy("db_slots_timeout"))?
            .map_err(unavailable)?;
            (None, queued, permit)
        };
        let queued_ms = started.elapsed().as_secs_f64() * 1000.0;
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _queued = queued;
            let _turn = turn;
            let pooled = state
                .pool
                .lock()
                .map_err(|_| Error::new("platform_unavailable", 503))?
                .pop();
            let db = match pooled {
                Some(db) => db,
                None => db::Store::open(state.config.clone(), state.key.clone())?,
            };
            let waiting = std::time::Instant::now();
            let lock_ms;
            let work_started;
            let result = if access == Access::Write {
                let _guard = state
                    .transition
                    .write()
                    .map_err(|_| Error::new("platform_unavailable", 503))?;
                lock_ms = waiting.elapsed().as_secs_f64() * 1000.0;
                work_started = std::time::Instant::now();
                // Invalidate before work, including errors after partially committed writes.
                match change {
                    CacheChange::All => {
                        state
                            .data_revision
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    CacheChange::Tenant(dsp, domain) => {
                        state.read_cache.invalidate_tenant(&dsp, domain)
                    }
                    CacheChange::Bookkeeping => {}
                }
                f(&db)
            } else {
                let _guard = state
                    .transition
                    .read()
                    .map_err(|_| Error::new("platform_unavailable", 503))?;
                lock_ms = waiting.elapsed().as_secs_f64() * 1000.0;
                work_started = std::time::Instant::now();
                f(&db)
            };
            let work_ms = work_started.elapsed().as_secs_f64() * 1000.0;
            // Any transaction in the work has committed, so the mailer finds what it queued,
            // the scheduler the deadlines it moved, and the live readers what was switched.
            if db.take_mail_queued() {
                state.mail_wake.notify_one();
            }
            if db.take_schedules_moved() {
                state
                    .schedule_revision
                    .fetch_add(1, std::sync::atomic::Ordering::Release);
            }
            if db.take_readers_moved() {
                state.readers_wake.notify_one();
            }
            if queued_ms + lock_ms + work_ms >= 25.0 {
                let context = trace
                    .as_ref()
                    .map(|trace| trace.lock().unwrap_or_else(|poison| poison.into_inner()));
                observability::event(
                    "info",
                    "database.slow",
                    serde_json::json!({
                        "requestId":context.as_ref().and_then(|c| c.request_id.as_deref()),
                        "work":format!("{}:{}", caller.file(), caller.line()),
                        "route":context.as_ref().and_then(|c| c.route),
                        "dspId":context.as_ref().and_then(|c| c.tenant.as_deref()),
                        "write":access != Access::Read,
                        "unpublished":access == Access::Unpublished, "queueMs":queued_ms, "lockMs":lock_ms, "workMs":work_ms,
                        "totalMs":started.elapsed().as_secs_f64()*1000.0
                    }),
                );
            }
            state
                .pool
                .lock()
                .map_err(|_| Error::new("platform_unavailable", 503))?
                .push(db);
            result
        })
        .await
        .map_err(|_| Error::new("operation_failed", 500))?
    }
}

pub async fn cancelled(receiver: &mut tokio::sync::watch::Receiver<bool>) {
    let _ = receiver.wait_for(|v| *v).await;
}

/// An essential background task ending unexpectedly must stop serving readiness.
/// Systemd can then restart the whole core and recover its durable jobs.
pub async fn supervise(
    task: impl std::future::Future<Output = Result<()>> + Send + 'static,
    stop: tokio::sync::watch::Sender<bool>,
) -> Result<()> {
    let result = tokio::spawn(task)
        .await
        .unwrap_or_else(|_| Err(Error::new("background_task_failed", 500)));
    if !*stop.borrow() {
        stop.send_replace(true);
        return Err(result
            .err()
            .unwrap_or_else(|| Error::new("background_task_stopped", 500)));
    }
    result
}

#[cfg(test)]
#[path = "../tests/backend/state.rs"]
mod tests;
