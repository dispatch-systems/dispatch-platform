#[path = "../../core/accounts/backend/mod.rs"]
pub mod accounts;
#[path = "../../core/mcp/backend/mod.rs"]
pub mod agents;
#[path = "../../core/tenancy/backend/audit.rs"]
pub mod audit;
#[path = "../../core/collection/backend/browser/mod.rs"]
pub mod browsers;
#[path = "cli.rs"]
pub mod cli;
#[path = "../../core/collection/backend/checkpoint.rs"]
pub mod collection_checkpoint;
#[path = "../../core/collection/backend/registry.rs"]
pub mod collectors;
#[path = "../../core/foundation/backend/config/mod.rs"]
pub mod config;
#[path = "contracts.rs"]
pub mod contracts;
#[path = "../../core/foundation/backend/crypto.rs"]
pub mod crypto;
#[path = "../../core/db/backend/mod.rs"]
pub mod db;
#[path = "../../features/driver_match/backend/mod.rs"]
pub mod driver_match;
#[path = "../../features/dvic/backend/mod.rs"]
pub mod dvic;
#[path = "../../core/foundation/backend/error.rs"]
pub mod error;
#[path = "../../core/tenancy/backend/catalog.rs"]
pub mod features;
#[path = "../../core/server/backend/http/mod.rs"]
pub mod http;
#[path = "../../core/collection/backend/metrics.rs"]
pub mod job_metrics;
#[path = "../../core/collection/backend/jobs/mod.rs"]
pub mod jobs;
#[path = "../../core/collection/backend/live.rs"]
pub mod live_collection;
#[path = "../../core/server/backend/live.rs"]
pub mod live_updates;
#[path = "../../core/server/backend/mail/mod.rs"]
pub mod mail;
#[path = "../../features/timecard/backend/meals/mod.rs"]
pub mod meals;
#[path = "../../core/foundation/backend/observability.rs"]
pub mod observability;
#[path = "../../core/server/backend/operations.rs"]
pub mod operations;
#[path = "../../core/server/backend/presence.rs"]
pub mod presence;
#[path = "../../core/server/backend/http/proxy.rs"]
pub mod proxy;
#[path = "../../core/server/backend/cache.rs"]
pub mod read_cache;
#[path = "../../core/tenancy/backend/roles.rs"]
pub mod roles;
#[path = "../../features/routes/backend/mod.rs"]
pub mod routedata;
#[path = "../../core/collection/backend/schedules.rs"]
pub mod schedules;
#[path = "../../features/scorecard/backend/mod.rs"]
pub mod scorecard;
#[path = "../../core/tenancy/backend/dsps.rs"]
pub mod tenants;
#[path = "../../features/uniforms/backend/mod.rs"]
pub mod uniforms;
#[path = "../../core/foundation/backend/validate.rs"]
pub mod validate;
#[path = "../../features/timecard/backend/punches/mod.rs"]
pub mod workforce;

pub use error::{Code, Error, Result, ensure};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Semaphore;

enum CacheChange {
    All,
    Bookkeeping,
    Tenant(String, read_cache::DataDomain),
}

pub struct State {
    pub config: config::Config,
    pub key: Vec<u8>,
    pub assets: std::collections::HashMap<String, http::Asset>,
    pub db_slots: Arc<Semaphore>,
    pub db_queue: Arc<Semaphore>,
    // Serializes short state transitions across the platform, jobs and tenant databases.
    pub transition: RwLock<()>,
    pub pool: Mutex<Vec<db::Store>>,
    pub read_cache: read_cache::ReadCache,
    pub data_revision: std::sync::atomic::AtomicU64,
    pub schedule_revision: std::sync::atomic::AtomicU64,
    pub password_slots: Arc<Semaphore>,
    pub mail_transport: Mutex<mail::TransportHealth>,
    // Wakes the mailer when a request queues mail, instead of it waiting for its next tick.
    pub mail_wake: tokio::sync::Notify,
    pub browsers: browsers::Manager,
    pub updates: live_updates::Updates,
    pub uniform_updates: live_updates::Updates,
    pub presence: presence::Presence,
    // How much each agent key is used, until the scheduler writes it down.
    pub agents: agents::Usage,
    // The calls agents made, until the scheduler writes them down.
    pub activity: agents::Activity,
    // The known apps' client documents, as last fetched.
    pub oauth: agents::oauth::Documents,
    // Public OAuth requests admitted before they can consume database capacity.
    pub oauth_limits: agents::oauth::limits::Limits,
}
impl State {
    pub fn new(config: config::Config) -> Result<Arc<Self>> {
        let store = db::Store::initialize(config.clone())?;
        for dsp in store.platform.all(
            "SELECT id FROM dsps WHERE status IN ('active','suspended')",
            [],
        )? {
            for provider in collectors::Provider::ALL {
                store.collector(db::s(&dsp, "id"), *provider)?.exec("UPDATE connections SET \
                    status='error',error='verification_expired' WHERE status IN ('signing_in','needs_verification')",[])?;
            }
        }
        // Each key's calls recorded today, so a restart keeps its daily cap.
        let activity = agents::Activity::seeded(&store)?;
        Ok(Arc::new(Self {
            key: store.key.clone(),
            assets: http::assets(&config.dashboard, &config.release)?,
            config,
            db_slots: Arc::new(Semaphore::new(4)),
            db_queue: Arc::new(Semaphore::new(64)),
            transition: RwLock::new(()),
            pool: Mutex::new(vec![store]),
            read_cache: read_cache::ReadCache::default(),
            data_revision: std::sync::atomic::AtomicU64::new(0),
            schedule_revision: std::sync::atomic::AtomicU64::new(0),
            password_slots: Arc::new(Semaphore::new(2)),
            mail_transport: Mutex::new(mail::TransportHealth::default()),
            mail_wake: tokio::sync::Notify::new(),
            browsers: browsers::Manager::default(),
            updates: live_updates::Updates::new()?,
            uniform_updates: live_updates::Updates::new()?,
            presence: presence::Presence::default(),
            agents: agents::Usage::default(),
            activity,
            oauth: agents::oauth::Documents::default(),
            oauth_limits: agents::oauth::limits::Limits::default(),
        }))
    }
    pub async fn run<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.database(true, CacheChange::All, f).await
    }
    /// Writes only reviewed bookkeeping: metrics, leases, outbox or invisible cleanup.
    /// Authorization still reads the database on every request; this skips derived-data
    /// invalidation, never the exclusive transition lock.
    pub async fn run_bookkeeping<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.database(true, CacheChange::Bookkeeping, f).await
    }
    /// A tenant mutation with known dependencies. Unknown writes must keep using `run`.
    pub async fn run_scoped<T: Send + 'static>(
        self: &Arc<Self>,
        dsp: impl Into<String>,
        domain: read_cache::DataDomain,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.database(true, CacheChange::Tenant(dsp.into(), domain), f)
            .await
    }
    pub async fn read<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.database(false, CacheChange::Bookkeeping, f).await
    }
    async fn database<T: Send + 'static>(
        self: &Arc<Self>,
        write: bool,
        change: CacheChange,
        f: impl FnOnce(&db::Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        // Bound both queued requests and blocking threads. Short bursts wait without
        // allocating another database pool or failing otherwise healthy requests.
        let started = std::time::Instant::now();
        let queued = self
            .db_queue
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new("platform_busy", 503))?;
        let permit = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.db_slots.clone().acquire_owned(),
        )
        .await
        .map_err(|_| Error::new("platform_busy", 503))?
        .map_err(|_| Error::new("platform_unavailable", 503))?;
        let queued_ms = started.elapsed().as_secs_f64() * 1000.0;
        let state = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _queued = queued;
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
            let result = if write {
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
            // Any transaction in the work has committed, so the mailer finds what it queued.
            if db.take_mail_queued() {
                state.mail_wake.notify_one();
            }
            if queued_ms + lock_ms + work_ms >= 25.0 {
                observability::event(
                    "info",
                    "database.slow",
                    serde_json::json!({
                        "write":write, "queueMs":queued_ms, "lockMs":lock_ms, "workMs":work_ms,
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
