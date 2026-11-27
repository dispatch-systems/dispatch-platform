mod admission;
pub mod attempt;
pub mod browseros;
mod driver;
pub mod egress;
mod fixture;
pub mod http;
pub mod page;
mod saved;
pub use crate::collection::registry::Provider;
use crate::{
    Error, Result, State,
    accounts::Context,
    collection::{api::types::Connection, metrics::SavedSession},
    db::{self, Store, s},
    ensure,
    foundation::crypto,
    tenancy::api::types::DspStatus,
};
pub use driver::{Collected, Driver, Pending, Run};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, watch};
#[derive(Default)]
pub struct Manager {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    operations: Mutex<HashMap<String, std::sync::Weak<AsyncMutex<()>>>>,
    runtime: Mutex<Option<Arc<browseros::Runtime>>>,
    /// Live readers' browsers closing on tasks of their own (`State::close_reader_browsers`),
    /// which the manager's own close waits for.
    closing: Mutex<tokio::task::JoinSet<()>>,
}
pub struct Session {
    pub id: String,
    pub dsp: String,
    pub provider: Provider,
    pub revision: i64,
    pub timezone: String,
    run: PathBuf,
    /// Its browser's profile, which it starts from once it needs one.
    profile: PathBuf,
    process_id: std::sync::atomic::AtomicU32,
    observed_pss: std::sync::atomic::AtomicU64,
    /// Its driver closed the browser before the session ended.
    browser_exited: std::sync::atomic::AtomicBool,
    status: AtomicU8, // 0 starting, 1 ready, 2 challenge, 3 closed
    worker: AsyncMutex<Option<Box<dyn Driver>>>,
    commands: tokio::sync::Semaphore,
    cancel: watch::Sender<bool>,
    // No browser runs in fixture mode, nor for a driver that needs none, nor while it reads
    // with the session a sign-in saved: nothing to take over, no memory to admit.
    browserless: AtomicBool,
    /// What became of the session a sign-in saved, for a job that started from one: while it
    /// is `Browserless`, the job reads with it and starts its browser once a collection needs
    /// a page.
    saved: Mutex<Option<SavedSession>>,
    started: std::time::Instant,
    last_used: Mutex<std::time::Instant>,
    collecting: std::sync::atomic::AtomicBool,
    /// The live reader whose hook opened it, by its id: its own, which no job's end closes,
    /// and which closes as the reader stops for the DSP.
    reader: Option<&'static str>,
    credential_lease: Mutex<Option<std::fs::File>>,
}

/// The durable authority behind a provider operation. Member requests, jobs and live
/// readers have different revocation rules, but none may reach credentials, a browser
/// command or provider state through an unguarded/default path.
#[derive(Clone)]
pub(crate) enum ProviderAuthority {
    Member(Box<Context>),
    Job {
        id: String,
        owner: String,
    },
    /// A live reader's hook, while its feature, by its switch, is on for the DSP.
    Reader {
        reader: &'static str,
        feature: &'static str,
    },
}
impl ProviderAuthority {
    fn check(&self, db: &Store, dsp: &str, provider: Provider) -> Result<()> {
        match self {
            Self::Member(context) => {
                let current = db.revalidate(context, "connections.manage")?;
                ensure(current.dsp.id == dsp, "permission_denied", 403)?;
                ensure(current.has(provider.id()), "not_found", 404)
            }
            Self::Job { id, owner } => {
                let current = db.guard(id, owner)?;
                ensure(current.id == dsp, "job_cancelled", 409)?;
                let job = db.job_row(id, Some(dsp))?;
                ensure(job.provider() == provider, "connection_changed", 409)
            }
            Self::Reader { feature, .. } => {
                db.ensure_dsp_active(dsp)?;
                ensure(db.feature_enabled(dsp, feature)?, "feature_disabled", 409)
            }
        }
    }
    /// The live reader it is, whose browser is its own alone: it never uses one another
    /// opened, and holds its own from the start, as a job does while it collects, so neither a
    /// job nor the sweep of idle browsers takes it.
    fn reader(&self) -> Option<&'static str> {
        match self {
            Self::Reader { reader, .. } => Some(reader),
            _ => None,
        }
    }
    pub(crate) async fn revalidate(
        &self,
        state: &Arc<State>,
        dsp: &str,
        provider: Provider,
    ) -> Result<()> {
        let authority = self.clone();
        let dsp = dsp.to_owned();
        state
            .read(move |db| authority.check(db, &dsp, provider))
            .await
    }
}
impl Manager {
    pub(crate) fn with_verified_runtime(runtime: browseros::Runtime) -> Self {
        Self {
            runtime: Mutex::new(Some(Arc::new(runtime))),
            ..Self::default()
        }
    }
    fn runtime(
        &self,
        config: &crate::foundation::config::Config,
    ) -> Result<Arc<browseros::Runtime>> {
        let mut current = self
            .runtime
            .lock()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        if let Some(runtime) = current.as_ref() {
            return Ok(runtime.clone());
        }
        let runtime = Arc::new(browseros::Runtime::new(
            &config.browseros,
            &config.sandbox,
            &std::env::current_exe()?,
            &config.environment_root().join("browser-runs"),
            config.browser_capacity,
        )?);
        *current = Some(runtime.clone());
        Ok(runtime)
    }
    pub fn operation(&self, id: &str) -> Result<tokio::sync::OwnedMutexGuard<()>> {
        let lock = {
            let mut operations = self
                .operations
                .lock()
                .map_err(|_| Error::new("browser_unavailable", 503))?;
            operations.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = operations.get(id).and_then(std::sync::Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(AsyncMutex::new(()));
                operations.insert(id.into(), Arc::downgrade(&lock));
                lock
            }
        };
        lock.try_lock_owned()
            .map_err(|_| Error::new("connection_busy", 409))
    }
    pub async fn revoke_current(&self, session: &Arc<Session>) {
        let removed = {
            let mut sessions = self.sessions.lock().expect("browser registry");
            if sessions
                .get(&session.provider.key(&session.dsp))
                .is_some_and(|current| Arc::ptr_eq(current, session))
            {
                sessions.remove(&session.provider.key(&session.dsp))
            } else {
                None
            }
        };
        if let Some(session) = removed {
            session.close().await;
        }
    }
    /// Closes a job's browser once the job ends: the DSP's for `provider` at the job's
    /// revision, unless a live reader opened it since.
    pub async fn revoke_provider_revision(&self, id: &str, revision: i64, provider: Provider) {
        if let Some(session) = self.get_for(id, provider)
            && session.revision == revision
            && session.reader.is_none()
        {
            self.revoke_current(&session).await;
        }
    }

    /// Closes the DSP's browsers that the live reader `reader` opened.
    async fn revoke_reader(&self, id: &str, reader: &str) {
        let opened: Vec<Arc<Session>> = self
            .sessions
            .lock()
            .map(|sessions| {
                sessions
                    .values()
                    .filter(|session| session.dsp == id && session.reader == Some(reader))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for session in opened {
            self.revoke_current(&session).await;
        }
    }
    pub fn get_for(&self, id: &str, provider: Provider) -> Option<Arc<Session>> {
        self.sessions.lock().ok()?.get(&provider.key(id)).cloned()
    }
    pub fn admission(&self) -> admission::Admission {
        let sessions = self.sessions.lock().expect("browser registry");
        admission::Admission::new(
            admission::available(),
            sessions.values().filter_map(|session| session.resident()),
        )
    }
    pub fn active(&self) -> usize {
        self.sessions.lock().map(|s| s.len()).unwrap_or(0)
    }
    pub async fn revoke(&self, id: &str) {
        for provider in Provider::all() {
            self.revoke_for(id, provider).await;
        }
    }
    pub async fn revoke_for(&self, id: &str, provider: Provider) {
        let session = self
            .sessions
            .lock()
            .ok()
            .and_then(|mut s| s.remove(&provider.key(id)));
        if let Some(session) = session {
            session.close().await;
        }
    }
    pub async fn close(&self) {
        let sessions = self
            .sessions
            .lock()
            .map(|mut s| s.drain().map(|(_, s)| s).collect::<Vec<_>>())
            .unwrap_or_default();
        for s in sessions {
            s.close().await;
        }
        let mut closing = std::mem::take(&mut *self.closing.lock().expect("browser registry"));
        while closing.join_next().await.is_some() {}
    }
}
impl Session {
    pub fn observe_memory(&self, memory: &crate::collection::metrics::Memory) {
        if memory.complete {
            self.observed_pss.store(memory.pss, Ordering::Release);
        }
    }
    /// The browser left while the session continues, as a collection reading over
    /// HTTP does; it no longer needs memory held for it.
    pub fn browser_exited(&self) {
        self.browser_exited.store(true, Ordering::Release);
    }
    /// What its browser last used, while it has one.
    fn resident(&self) -> Option<u64> {
        (!self.browserless() && !self.browser_exited.load(Ordering::Acquire))
            .then(|| self.observed_pss.load(Ordering::Acquire))
    }
    pub fn process_id(&self) -> Option<u32> {
        let id = self.process_id.load(Ordering::Acquire);
        (id != 0 && !self.closed()).then_some(id)
    }

    pub fn ready(&self) -> bool {
        self.status.load(Ordering::SeqCst) == 1
    }
    pub fn challenge(&self) -> bool {
        self.status.load(Ordering::SeqCst) == 2
    }
    pub fn closed(&self) -> bool {
        self.status.load(Ordering::SeqCst) == 3
    }
    pub fn interactive(&self) -> bool {
        self.challenge() && !self.browserless()
    }
    fn browserless(&self) -> bool {
        self.browserless.load(Ordering::Acquire)
    }
    /// What became of the session a sign-in saved, for a job that started from one.
    pub fn saved_session(&self) -> Option<SavedSession> {
        *self.saved.lock().expect("saved session")
    }
    /// It reads with the session a sign-in saved, with no browser yet.
    fn saved(&self) -> bool {
        self.saved_session() == Some(SavedSession::Browserless)
    }
    fn became(&self, saved: SavedSession) {
        *self.saved.lock().expect("saved session") = Some(saved);
    }
    pub fn busy(&self) -> bool {
        self.status.load(Ordering::SeqCst) == 0
            || self.worker.try_lock().is_err()
            || self.collecting.load(Ordering::SeqCst)
    }
    async fn close(&self) {
        self.status.store(3, Ordering::SeqCst);
        self.cancel.send_replace(true);
        if let Some(worker) = self.worker.lock().await.take()
            && let Some(browser) = worker.browser()
        {
            browser.close().await;
        }
        let _ = std::fs::remove_dir_all(&self.run);
        self.credential_lease
            .lock()
            .expect("credential lease")
            .take();
    }
    pub(crate) async fn request_guarded(
        &self,
        command: Value,
        types: &[&str],
        seconds: u64,
        state: &Arc<State>,
        authority: &ProviderAuthority,
    ) -> Result<Value> {
        ensure(!self.closed(), "verification_expired", 409)?;
        let _slot = self
            .commands
            .try_acquire()
            .map_err(|_| Error::new("connection_busy", 409))?;
        let mut cancellation = self.cancel.subscribe();
        let mut worker = tokio::select! {
            _=crate::cancelled(&mut cancellation)=>return Err(Error::new("verification_expired",409)),
            lock=tokio::time::timeout(Duration::from_secs(seconds),
                self.worker.lock())=>lock.map_err(|_|Error::new("provider_timeout",504))?,
        };
        ensure(!self.closed(), "verification_expired", 409)?;
        authority
            .revalidate(state, &self.dsp, self.provider)
            .await?;
        if ["screenshot", "assist", "complete_assistance"].contains(&s(&command, "action")) {
            ensure(self.interactive(), "verification_expired", 409)?;
        }
        *self.last_used.lock().expect("browser idle clock") = std::time::Instant::now();
        let worker = worker
            .as_mut()
            .ok_or_else(|| Error::new("browser_unavailable", 409))?;
        let mut cancellation = self.cancel.subscribe();
        let response = worker.request(command);
        let event = tokio::select! {
            _=cancellation.wait_for(|closed|*closed)=>Err(Error::new("verification_expired",409)),
            result=tokio::time::timeout(Duration::from_secs(seconds),response)=>result.map_err(|_|Error::new("provider_timeout",504))?,
        }?;
        ensure(
            types.contains(&s(&event, "type")),
            "browser_protocol_failed",
            502,
        )?;
        // A provider response can arrive after a member, job or feature was
        // revoked. Refuse it before changing the shared session or returning it.
        authority
            .revalidate(state, &self.dsp, self.provider)
            .await?;
        *self.last_used.lock().expect("browser idle clock") = std::time::Instant::now();
        if s(&event, "type") == "ready" {
            self.status.store(1, Ordering::SeqCst);
            self.keep_session(&mut **worker, state, authority).await;
        }
        if s(&event, "type") == "challenge" {
            self.status.store(2, Ordering::SeqCst);
        }
        Ok(event)
    }
    /// Saves the provider's session as `driver` holds it, for the next job to start from with
    /// no browser. One left unsaved only means the next job starts a browser.
    async fn keep_session(
        &self,
        driver: &mut dyn Driver,
        state: &Arc<State>,
        authority: &ProviderAuthority,
    ) {
        let Ok(Some(session)) = driver.session().await else {
            return;
        };
        let (dsp, provider, revision) = (self.dsp.clone(), self.provider, self.revision);
        let authority = authority.clone();
        let saved = state
            .run_bookkeeping(move |db| {
                authority.check(db, &dsp, provider)?;
                db.save_session(&dsp, provider, revision, &session)
            })
            .await;
        if let Err(error) = saved {
            crate::foundation::observability::event(
                "warn",
                "browser.session_unsaved",
                json!({"provider":provider.id(),"error":error.code}),
            );
        }
    }
    /// Waits while its start waits on the owner's verification, as the job's progress says,
    /// until it is ready, failing once it closes.
    pub(crate) async fn verified(
        &self,
        state: &Arc<State>,
        job: &str,
        owner: &str,
        metrics: &crate::collection::metrics::Recorder,
    ) -> Result<()> {
        if !self.challenge() {
            return Ok(());
        }
        metrics.phase(crate::collection::metrics::Phase::Verification);
        let (job, owner) = (job.to_owned(), owner.to_owned());
        state
            .run_bookkeeping(move |db| {
                db.progress(
                    &job,
                    &owner,
                    5,
                    "Waiting for owner verification",
                    crate::collection::api::jobs::ActiveJobStatus::WaitingVerification,
                )
            })
            .await?;
        while !self.ready() {
            ensure(!self.closed(), "verification_expired", 409)?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }
    pub async fn collect(
        self: &Arc<Self>,
        state: &Arc<State>,
        job: &str,
        owner: &str,
        metrics: &crate::collection::metrics::Recorder,
        request: &Value,
        attempt: i64,
    ) -> Result<Collected> {
        ensure(self.ready(), "verification_required", 409)?;
        ensure(
            !self.collecting.swap(true, Ordering::SeqCst),
            "connection_busy",
            409,
        )?;
        let run = Run {
            state,
            job,
            owner,
            timezone: &self.timezone,
            metrics,
            request,
            attempt,
        };
        let authority = ProviderAuthority::Job {
            id: job.into(),
            owner: owner.into(),
        };
        loop {
            match self.collect_once(&run, &authority).await {
                // A collection needed a page: the job's browser starts now, as it would have
                // with no saved session, and the collection runs again in it.
                Err(error) if error.is(crate::Code::BrowserRequired) && self.saved() => {
                    self.became(SavedSession::PageNeeded);
                    metrics.saved_session(SavedSession::PageNeeded);
                    metrics.phase(crate::collection::metrics::Phase::Authentication);
                    state.browser_for(self, &authority).await?;
                    self.verified(state, job, owner, metrics).await?;
                    metrics.phase(crate::collection::metrics::Phase::Collection);
                }
                collected => return collected,
            }
        }
    }
    /// One collection by its driver as it is now, its session saved once it succeeds: what
    /// its replies set is what the next job starts from.
    async fn collect_once(
        &self,
        run: &Run<'_>,
        authority: &ProviderAuthority,
    ) -> Result<Collected> {
        let mut worker = self.worker.lock().await;
        let worker = worker
            .as_mut()
            .ok_or_else(|| Error::new("browser_unavailable", 409))?;
        let mut cancellation = self.cancel.subscribe();
        let response = worker.collect(run);
        let collected = tokio::select! {
            _=cancellation.wait_for(|closed|*closed)=>Err(Error::new("job_cancelled",409)),
            result=tokio::time::timeout(Duration::from_secs(1800),response)=>result.map_err(|_|Error::new("provider_timeout",504))?,
        };
        if collected.is_ok() {
            self.keep_session(&mut **worker, run.state, authority).await;
        }
        collected
    }
}
/// Whether a browser may start beside those `sessions` hold: within the server's capacity,
/// and with the memory each needs. A session with no browser counts toward neither.
fn admit(sessions: &HashMap<String, Arc<Session>>, capacity: usize) -> Result<()> {
    let browsers = sessions.values().filter(|s| !s.browserless()).count();
    ensure(browsers < capacity, "browser_capacity_busy", 429)?;
    let admission = admission::Admission::new(
        admission::available(),
        sessions.values().filter_map(|s| s.resident()),
    );
    ensure(admission.can_start, "browser_memory_busy", 503)
}
/// A browser session that closes when dropped, unless kept: one a start registered until
/// it's done, or a live reader's until its hook is done with it.
struct Closing {
    state: Arc<State>,
    session: Option<Arc<Session>>,
}
impl Closing {
    fn new(state: &Arc<State>, session: Arc<Session>) -> Self {
        Self {
            state: state.clone(),
            session: Some(session),
        }
    }
    fn session(&self) -> &Arc<Session> {
        self.session.as_ref().expect("open until it closes")
    }
    fn keep(mut self) -> Arc<Session> {
        self.session.take().expect("open until it closes")
    }
    async fn close(&mut self) {
        if let Some(session) = self.session.take() {
            self.state.browsers.revoke_current(&session).await;
        }
    }
}
impl Drop for Closing {
    fn drop(&mut self) {
        if let Some(session) = self.session.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let state = self.state.clone();
            runtime.spawn(async move { state.browsers.revoke_current(&session).await });
        }
    }
}

/// A DSP's browser for a provider that a live reader's hook opened outside a job
/// (`readers::Reading::browser`), signed in and its alone: closed when dropped, and as the
/// reader stops for the DSP.
pub struct ProviderBrowser(Closing);
impl ProviderBrowser {
    pub(crate) async fn open(
        state: &Arc<State>,
        dsp: &str,
        provider: Provider,
        reader: &'static str,
        feature: &'static str,
    ) -> Result<Self> {
        let authority = ProviderAuthority::Reader { reader, feature };
        let session = state
            .ensure_provider_browser(dsp, false, provider, authority)
            .await?;
        let browser = Self(Closing::new(state, session));
        // Nobody is there to finish signing in: it closes rather than wait for the owner.
        ensure(browser.0.session().ready(), "verification_required", 409)?;
        Ok(browser)
    }
    /// The browser, signed in, for what the hook takes from it: none in fixture mode, where
    /// no browser runs, and once it's closed.
    pub async fn browser(&self) -> Option<browseros::Session> {
        let worker = self.0.session().worker.lock().await;
        worker.as_ref().and_then(|driver| driver.browser().cloned())
    }
    /// Closes it, as a job's closes once it ends.
    pub async fn close(mut self) {
        self.0.close().await;
    }
}

impl Store {
    /// Credential changes reset only this collector's sessions, the one a sign-in saved with
    /// them. Call after its browser worker closes; other collectors' profiles must survive
    /// unchanged.
    pub fn clear_collector_browser_state(&self, id: &str, provider: Provider) -> Result<()> {
        let browsers = self.area(id, "state")?.join("browsers");
        if !browsers.try_exists()? {
            return Ok(());
        }
        self.discard_session(id, provider)?;
        db::private_dir(&browsers)?;
        for entry in provider.collector().browser_entries() {
            let path = browsers.join(entry);
            match std::fs::symlink_metadata(&path) {
                Ok(stat) if stat.is_dir() => {
                    db::private_dir(&path)?;
                    std::fs::remove_dir_all(path)?;
                }
                Ok(_) => {
                    db::private_file(&path, false)?;
                    std::fs::remove_file(path)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
impl State {
    /// Closes the DSP's browsers that the live reader `reader` opened, as it stops reading
    /// there, on a task of its own: what a hook still holds of one fails from then on, and the
    /// caller waits on none of them, as one still starting would keep it waiting.
    pub(crate) fn close_reader_browsers(self: &Arc<Self>, dsp: &str, reader: &'static str) {
        let (state, dsp) = (self.clone(), dsp.to_owned());
        let mut closing = self.browsers.closing.lock().expect("browser registry");
        while closing.try_join_next().is_some() {}
        closing.spawn(async move { state.browsers.revoke_reader(&dsp, reader).await });
    }
    pub async fn expire_browsers(self: &Arc<Self>) {
        let ids = self
            .browsers
            .sessions
            .lock()
            .map(|s| {
                s.values()
                    .filter(|s| {
                        s.closed()
                            || (s.ready()
                                && !s.busy()
                                && s.last_used
                                    .lock()
                                    .is_ok_and(|time| time.elapsed() > Duration::from_secs(60)))
                            || s.started.elapsed()
                                > Duration::from_secs(if s.collecting.load(Ordering::SeqCst) {
                                    1800
                                } else {
                                    600
                                })
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for session in ids {
            let expired = session.challenge() || !session.ready();
            self.browsers.revoke_current(&session).await;
            if expired {
                let id = session.dsp.clone();
                let revision = session.revision;
                let provider = session.provider;
                let _ = self
                    .run_scoped(
                        session.dsp.clone(),
                        crate::server::cache::DataDomain::TENANT,
                        move |db| {
                            db.connection_state(
                                &id,
                                provider,
                                revision,
                                "error",
                                Some("verification_expired"),
                            )
                        },
                    )
                    .await;
            }
        }
    }
    pub(crate) async fn connection(
        self: &Arc<Self>,
        context: &Context,
        provider: Provider,
    ) -> Result<Connection> {
        let authority = ProviderAuthority::Member(Box::new(context.clone()));
        let dsp = context.dsp.id.clone();
        let session = self.browsers.get_for(&dsp, provider);
        self.read(move |db| {
            authority.check(db, &dsp, provider)?;
            let mut value = db.connection_for(&dsp, provider)?;
            if let Some(session) = session
                && session.interactive()
            {
                value.verification_session_id = Some(session.id.clone());
            }
            Ok(value)
        })
        .await
    }
    pub(crate) async fn ensure_provider_browser(
        self: &Arc<Self>,
        id: &str,
        retry: bool,
        provider: Provider,
        authority: ProviderAuthority,
    ) -> Result<Arc<Session>> {
        let dsp = id.to_owned();
        let initial_authority = authority.clone();
        let fixture = self.config.fixture && self.config.fixture_url.is_none();
        let (dsp, credentials, revision, run, profile, direct, saved) = self
            .run_bookkeeping(move |db| {
                initial_authority.check(db, &dsp, provider)?;
                let value = db.ensure_dsp_active(&dsp)?;
                let connection = db
                    .connection_lease(&dsp, provider)?
                    .ok_or_else(|| Error::new("connection_required", 409))?;
                ensure(connection.enabled, "connection_required", 409)?;
                let runs = db::private_dir(&db.config.environment_root().join("browser-runs"))?;
                let run = runs.join(crypto::id("run")?);
                let profile = db::private_dir(&db.area(&dsp, "state")?.join("browsers"))?
                    .join(format!("{}-browseros", provider.id()));
                let credentials = db.credentials(&dsp, provider)?;
                // The collector's own driver, when these credentials need no browser.
                let direct = provider.collector().direct(db, &dsp, &credentials)?;
                // The session a sign-in saved, for a job whose collection may read with it and
                // start no browser. An owner's retry signs in as always.
                let saved = match &initial_authority {
                    ProviderAuthority::Job { id, .. }
                        if direct.is_none()
                            && !retry
                            && !fixture
                            && provider
                                .collector()
                                .resumes(db.job_row(id, Some(&dsp))?.kind.as_str()) =>
                    {
                        db.saved_session(&dsp, provider, connection.revision)?
                    }
                    _ => None,
                };
                Ok((
                    value,
                    credentials,
                    connection.revision,
                    run,
                    profile,
                    direct,
                    saved,
                ))
            })
            .await?;
        if let Some(session) = self.browsers.get_for(id, provider) {
            ensure(
                authority.reader().is_none() && !session.closed() && !session.busy(),
                "connection_busy",
                409,
            )?;
            ensure(session.revision == revision, "connection_changed", 409)?;
            if retry {
                let result = session
                    .request_guarded(
                        json!({"action":"check","credentials":credentials}),
                        &["ready", "challenge"],
                        180,
                        self,
                        &authority,
                    )
                    .await;
                let persisted = self.browser_result(&session, &result, &authority).await;
                if result.is_err() || persisted.is_err() {
                    // An existing browser may belong to another authorized member
                    // or job. A caller losing authority must not tear it down.
                    if authority
                        .revalidate(self, &session.dsp, session.provider)
                        .await
                        .is_ok()
                    {
                        self.browsers.revoke_current(&session).await;
                    }
                }
                persisted?;
                result?;
            }
            return Ok(session);
        }
        let credential_lease = self
            .read({
                let dsp = id.to_owned();
                move |db| db.browser_lease(&dsp, provider)
            })
            .await?;
        let (cancel, _) = watch::channel(false);
        let session = Arc::new(Session {
            id: run.file_name().unwrap().to_string_lossy().into_owned(),
            dsp: id.into(),
            provider,
            revision,
            timezone: dsp.timezone.clone(),
            run: run.clone(),
            profile: profile.clone(),
            process_id: std::sync::atomic::AtomicU32::new(0),
            observed_pss: std::sync::atomic::AtomicU64::new(0),
            browser_exited: AtomicBool::new(false),
            status: AtomicU8::new(0),
            worker: AsyncMutex::new(None),
            commands: tokio::sync::Semaphore::new(32),
            cancel,
            browserless: AtomicBool::new(direct.is_some() || fixture || saved.is_some()),
            saved: Mutex::new(saved.is_some().then_some(SavedSession::Browserless)),
            started: std::time::Instant::now(),
            last_used: Mutex::new(std::time::Instant::now()),
            collecting: AtomicBool::new(authority.reader().is_some()),
            reader: authority.reader(),
            credential_lease: Mutex::new(Some(credential_lease)),
        });
        {
            let mut sessions = self
                .browsers
                .sessions
                .lock()
                .map_err(|_| Error::new("browser_unavailable", 503))?;
            ensure(
                !sessions.contains_key(&provider.key(id)),
                "connection_busy",
                409,
            )?;
            if !session.browserless() {
                admit(&sessions, self.config.browser_capacity)?;
            }
            sessions.insert(provider.key(id), session.clone());
        }
        // Registered, it closes unless the start ends with it ready to use: a start dropped
        // partway, as a read cut off or switched off, leaves nothing behind.
        let mut starting = Closing::new(self, session.clone());
        // What a driver without a browser is about to ask, each still waiting.
        let asking = direct.as_ref().map(|driver| driver.checks());
        let command = self.start_command(&credentials, &session.timezone, retry);
        let start = async {
            let mut worker = session.worker.lock().await;
            ensure(!session.closed(), "verification_expired", 409)?;
            let dsp = id.to_owned();
            let launch_authority = authority.clone();
            self.run_scoped(id, crate::server::cache::DataDomain::TENANT, move |db| {
                launch_authority.check(db, &dsp, provider)?;
                let tenant = db.find_dsp(&dsp)?;
                ensure(tenant.status == DspStatus::Active, "dsp_unavailable", 409)?;
                let connection = db
                    .connection_lease(&dsp, provider)?
                    .ok_or_else(|| Error::new("connection_required", 409))?;
                let current = connection.enabled && connection.revision == revision;
                ensure(current, "connection_changed", 409)?;
                db.connection_state(&dsp, provider, revision, "signing_in", None)?;
                match &asking {
                    Some(checks) => db.connection_checks(&dsp, provider, revision, checks),
                    None => Ok(()),
                }
            })
            .await?;
            ensure(!session.closed(), "verification_expired", 409)?;
            db::private_dir(&run)?;
            if let Some(driver) = direct {
                *worker = Some(driver);
            } else if fixture {
                db::private_dir(&profile)?;
                *worker = Some(Box::new(fixture::Driver::new(provider)));
            } else {
                db::private_dir(&profile)?;
                attempt::preflight(&profile, provider.id(), retry)?;
                *worker = Some(match saved {
                    Some(saved) => {
                        provider
                            .collector()
                            .resumed(saved, &profile, self.config.fixture_url.as_deref())
                            .await?
                    }
                    None => self.browser_driver(&session).await?,
                });
            }
            drop(worker);
            let started = session
                .request_guarded(
                    command.clone(),
                    &["ready", "challenge"],
                    180,
                    self,
                    &authority,
                )
                .await;
            match started {
                // The provider no longer accepts the saved session: it goes, and the browser
                // signs in as with none.
                Err(error) if error.is(crate::Code::BrowserRequired) && session.saved() => {
                    session.became(SavedSession::Refused);
                    let dsp = id.to_owned();
                    self.run_bookkeeping(move |db| db.discard_session(&dsp, provider))
                        .await?;
                    self.launch(&session, retry).await?;
                    session
                        .request_guarded(command, &["ready", "challenge"], 180, self, &authority)
                        .await
                }
                started => started,
            }
        }
        .await;
        let persisted = self.browser_result(&session, &start, &authority).await;
        if start.is_err() || persisted.is_err() {
            starting.close().await;
        }
        persisted?;
        start?;
        Ok(starting.keep())
    }
    /// What a start asks of a session's driver.
    fn start_command(&self, credentials: &Value, timezone: &str, retry: bool) -> Value {
        json!({"action":"start","credentials":credentials,"timezone":timezone,
            "ownerRetry":retry,"fixtureUrl":self.config.fixture_url})
    }
    /// A browser for `session`, started from its profile, and its collector's driver on it.
    async fn browser_driver(self: &Arc<Self>, session: &Session) -> Result<Box<dyn Driver>> {
        let provider = session.provider;
        let policy = if let Some(value) = &self.config.fixture_url {
            browseros::NetworkPolicy::Fixture(
                std::num::NonZeroU16::new(
                    url::Url::parse(value)
                        .expect("validated fixture URL")
                        .port()
                        .unwrap(),
                )
                .unwrap(),
            )
        } else {
            provider.collector().network()
        };
        let runtime = self.browsers.runtime(&self.config)?;
        let browser = runtime
            .start(&session.profile, browseros::Mode::Windowed, policy)
            .await?;
        session
            .process_id
            .store(browser.process_id(), Ordering::Release);
        match provider
            .collector()
            .driver(
                browser.clone(),
                &session.profile,
                self.config.fixture_url.as_deref(),
            )
            .await
        {
            Ok(driver) => Ok(driver),
            Err(error) => {
                browser.close().await;
                Err(error)
            }
        }
    }
    /// Starts the browser of a session that read with the session a sign-in saved until now:
    /// admitted as any browser is, from its profile, its driver in place of the one it read
    /// with.
    async fn launch(self: &Arc<Self>, session: &Arc<Session>, retry: bool) -> Result<()> {
        {
            let sessions = self
                .browsers
                .sessions
                .lock()
                .map_err(|_| Error::new("browser_unavailable", 503))?;
            admit(&sessions, self.config.browser_capacity)?;
            session.browserless.store(false, Ordering::Release);
        }
        attempt::preflight(&session.profile, session.provider.id(), retry)?;
        let mut worker = session.worker.lock().await;
        ensure(!session.closed(), "verification_expired", 409)?;
        *worker = Some(self.browser_driver(session).await?);
        Ok(())
    }
    /// Starts the browser of a job's `session` that read with the session a sign-in saved,
    /// once a collection needs a page, and signs it in as a job's start does.
    pub(crate) async fn browser_for(
        self: &Arc<Self>,
        session: &Arc<Session>,
        authority: &ProviderAuthority,
    ) -> Result<()> {
        let started = async {
            self.launch(session, false).await?;
            let (dsp, provider, check) = (session.dsp.clone(), session.provider, authority.clone());
            let credentials = self
                .read(move |db| {
                    check.check(db, &dsp, provider)?;
                    db.credentials(&dsp, provider)
                })
                .await?;
            let command = self.start_command(&credentials, &session.timezone, false);
            session
                .request_guarded(command, &["ready", "challenge"], 180, self, authority)
                .await
        }
        .await;
        let persisted = self.browser_result(session, &started, authority).await;
        persisted?;
        started.map(drop)
    }
    pub(crate) async fn browser_result(
        self: &Arc<Self>,
        session: &Arc<Session>,
        result: &Result<Value>,
        authority: &ProviderAuthority,
    ) -> Result<()> {
        let dsp = session.dsp.clone();
        let revision = session.revision;
        let provider = session.provider;
        let recoverable = result
            .as_ref()
            .is_err_and(|e| e.is_any(crate::Code::RECOVERABLE));
        let status = if result.is_ok() || recoverable {
            if session.ready() {
                "ready"
            } else {
                "needs_verification"
            }
        } else {
            "error"
        };
        let error = if recoverable {
            None
        } else {
            result.as_ref().err().map(|e| e.code.clone())
        };
        // What a driver without a browser found, line by line. One busy collecting is not
        // waited for: its connection was checked before it began.
        let checks = match session.worker.try_lock() {
            Ok(worker) => worker.as_ref().map(|driver| driver.checks()),
            Err(_) => None,
        }
        .filter(|checks| !checks.is_empty());
        let authority = authority.clone();
        self.run_scoped(
            dsp.clone(),
            crate::server::cache::DataDomain::TENANT,
            move |db| {
                authority.check(db, &dsp, provider)?;
                db.connection_state(&dsp, provider, revision, status, error.as_deref())?;
                match &checks {
                    Some(checks) => db.connection_checks(&dsp, provider, revision, checks),
                    None => Ok(()),
                }
            },
        )
        .await
    }
}

#[cfg(test)]
#[path = "../../tests/backend/browser/sessions.rs"]
mod tests;
