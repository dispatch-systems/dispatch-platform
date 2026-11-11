//! What the server shares across requests and background tasks.
use crate::{
    Error, Result,
    collection::browser,
    db,
    foundation::{config, observability},
    manifest,
    server::{admission, cache, http, live, mail, presence, startup},
};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

tokio::task_local! {
    /// Set while a claimed job runs: see `State::patiently`.
    static PATIENT: ();
    /// Repeated server-owned polls share bounded capacity with background work.
    static POLLING: ();
}
/// How long a running job's database call waits for its turn: past the longest write it may
/// queue behind, such as another DSP's publication, where a request gives up after
/// `WORKER_WAIT`.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);
/// How long a request waits for its workers, the routine share's and the database's together.
const WORKER_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
/// The database's workers, and the callers that may wait for one.
const DB_SLOTS: usize = 4;
const DB_QUEUE: usize = 64;
/// How many of the database's slots work outside a request holds at once. The rest are left to
/// requests, so every DSP's upkeep starting together keeps no page waiting behind it.
const BACKGROUND_SLOTS: usize = 2;
/// The routine callers that may wait for the routine share, before joining the database's
/// queue: one waiting for its own worker takes no entry there, so the two queues hold at most
/// `ROUTINE_QUEUE + DB_QUEUE` callers.
const ROUTINE_QUEUE: usize = 56;
/// The workers routine callers share, of the database's `DB_SLOTS`.
const ROUTINE_SLOTS: usize = 3;

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

/// What a database turn holds from its admission until its work ends, let go in this order.
struct Admitted {
    _background: Option<OwnedSemaphorePermit>,
    _queued: OwnedSemaphorePermit,
    _routine_queued: Option<OwnedSemaphorePermit>,
    _routine_worker: Option<OwnedSemaphorePermit>,
    _worker: OwnedSemaphorePermit,
}

pub struct State {
    pub config: config::Config,
    pub key: Vec<u8>,
    pub assets: std::collections::HashMap<String, http::Asset>,
    pub db_slots: Arc<Semaphore>,
    /// The turns at `db_slots` work outside a request takes: `BACKGROUND_SLOTS` at once.
    pub background_slots: Arc<Semaphore>,
    pub db_queue: Arc<Semaphore>,
    routine_queue: Arc<Semaphore>,
    routine_slots: Arc<Semaphore>,
    // Serializes short state transitions across the platform, jobs and tenant databases.
    pub transition: RwLock<()>,
    pub pool: Mutex<Vec<db::Store>>,
    pub read_cache: cache::ReadCache,
    /// Identical expensive reads take turns before entering the database pool. The next
    /// reader checks authorization and the cache again, so neither credentials nor errors
    /// are shared.
    pub read_turns: admission::KeyedTurns,
    // Unpublished writers serialize by DSP before taking any database worker.
    unpublished_turns: admission::KeyedTurns,
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
            assets: http::assets(&config.dashboard, &config.release, config.environment)?,
            config,
            db_slots: Arc::new(Semaphore::new(DB_SLOTS)),
            background_slots: Arc::new(Semaphore::new(BACKGROUND_SLOTS)),
            db_queue: Arc::new(Semaphore::new(DB_QUEUE)),
            routine_queue: Arc::new(Semaphore::new(ROUTINE_QUEUE)),
            routine_slots: Arc::new(Semaphore::new(ROUTINE_SLOTS)),
            transition: RwLock::new(()),
            pool: Mutex::new(vec![store]),
            read_cache: cache::ReadCache::default(),
            read_turns: admission::KeyedTurns::new(admission::Origins::READS),
            unpublished_turns: admission::KeyedTurns::new(admission::Origins::UNPUBLISHED),
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
    /// Lets go of what the server's caches and stores hold of a deleted DSP: its live channels,
    /// its cached reads, and the connections each idle store keeps to its databases. A store at work keeps its
    /// own until it's least recently used, as it can't open them again.
    pub fn forget_dsp(&self, dsp: &str) {
        self.topics.forget_dsp(dsp);
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
    /// Repeated polling leaves part of the existing database capacity for foreground
    /// requests. The server selects this scope; callers supply no priority or authority.
    /// Idle network waits hold no database permits. Each new turn still authorizes afresh.
    pub async fn polling<F: std::future::Future>(work: F) -> F::Output {
        POLLING.scope((), work).await
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
    /// Admits one database turn. Work outside a request (`background`) has no one to try it
    /// again, so it waits at every limiter as a job's does, for up to `PATIENCE`, and takes one
    /// of the background turns first, so it never holds every slot; so does a request's work
    /// under `patiently`. Any other request is turned away where a queue is full, and waits
    /// `WORKER_WAIT` in all for its workers. Routine work, in the background or polling, takes
    /// its own queue entry and worker first, and holds no entry of the database's queue while
    /// it waits for them.
    async fn admit(&self, background: bool) -> Result<Admitted> {
        let unavailable = |_| Error::new("platform_unavailable", 503);
        let routine = background || POLLING.try_with(|_| ()).is_ok();
        if background || PATIENT.try_with(|_| ()).is_ok() {
            return tokio::time::timeout(PATIENCE, async {
                let background = match background {
                    true => Some(self.background_slots.clone().acquire_owned().await?),
                    false => None,
                };
                let routine_queued = match routine {
                    true => Some(self.routine_queue.clone().acquire_owned().await?),
                    false => None,
                };
                let routine_worker = match routine {
                    true => Some(self.routine_slots.clone().acquire_owned().await?),
                    false => None,
                };
                let queued = self.db_queue.clone().acquire_owned().await?;
                let worker = self.db_slots.clone().acquire_owned().await?;
                Ok(Admitted {
                    _background: background,
                    _queued: queued,
                    _routine_queued: routine_queued,
                    _routine_worker: routine_worker,
                    _worker: worker,
                })
            })
            .await
            .map_err(|_| Error::busy("db_patient_timeout"))?
            .map_err(unavailable);
        }
        let routine_queued = match routine {
            true => Some(admission::slot(
                &self.routine_queue,
                "db_routine_queue_full",
                "db_routine_queue_closed",
            )?),
            false => None,
        };
        let until = tokio::time::Instant::now() + WORKER_WAIT;
        let routine_worker = match routine {
            true => Some(
                tokio::time::timeout_at(until, self.routine_slots.clone().acquire_owned())
                    .await
                    .map_err(|_| Error::busy("db_routine_slots_timeout"))?
                    .map_err(unavailable)?,
            ),
            false => None,
        };
        let queued = admission::slot(&self.db_queue, "db_queue_full", "db_queue_closed")?;
        let worker = tokio::time::timeout_at(until, self.db_slots.clone().acquire_owned())
            .await
            .map_err(|_| Error::busy("db_slots_timeout"))?
            .map_err(unavailable)?;
        Ok(Admitted {
            _background: None,
            _queued: queued,
            _routine_queued: routine_queued,
            _routine_worker: routine_worker,
            _worker: worker,
        })
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
        let admitted = self.admit(trace.is_none()).await?;
        let queued_ms = started.elapsed().as_secs_f64() * 1000.0;
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            let _admitted = admitted;
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
