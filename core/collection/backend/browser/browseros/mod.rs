//! Internal Rust browser runtime. Provider adapters own these handles; no raw CDP
//! endpoint, client-supplied path, or script is exposed through the platform API.
#[path = "cdp.rs"]
mod cdp;
#[path = "loading.rs"]
mod loading;
#[path = "native.rs"]
mod native;
#[path = "sandbox.rs"]
mod sandbox;
#[path = "worker.rs"]
mod worker;

use super::egress::Egress;
use crate::{Error, Result, crypto, db, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Semaphore, mpsc, oneshot, watch},
    time::{Instant, timeout, timeout_at},
};

const COMMAND_BYTES: u64 = 64 * 1024;
const RESPONSE_BYTES: u64 = cdp::MAX_FRAME + 1024;
const QUEUE_SIZE: usize = 8;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub enum Mode {
    Headless,
    Windowed,
}
impl Mode {
    fn argument(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Windowed => "windowed",
        }
    }
}

/// Chosen by trusted host code, never deserialized from a DSP request.
#[derive(Clone, Copy)]
pub enum NetworkPolicy {
    Paycom,
    Cortex,
    /// Synthetic local server, reachable only as fixture.dispatch.invalid.
    Fixture(std::num::NonZeroU16),
}

pub struct Runtime {
    browser: PathBuf,
    sandbox: PathBuf,
    executable: PathBuf,
    runs: PathBuf,
    slots: Arc<Semaphore>,
}
impl Runtime {
    pub fn new(
        browser: &Path,
        sandbox: &Path,
        executable: &Path,
        runs: &Path,
        capacity: usize,
    ) -> Result<Self> {
        ensure(
            (1..=16).contains(&capacity),
            "invalid_browser_capacity",
            400,
        )?;
        let browser = sandbox::trusted(browser, true)?;
        ensure(
            browser.file_name().is_some_and(|s| s == "browseros"),
            "browseros_required",
            503,
        )?;
        let executable = sandbox::trusted(executable, false)?;
        Ok(Self {
            browser,
            sandbox: sandbox::trusted(sandbox, true)?,
            executable,
            runs: db::private_dir(runs)?,
            slots: Arc::new(Semaphore::new(capacity)),
        })
    }

    /// Profile paths are derived by the host from its DSP registry. Only this
    /// profile and the egress socket are mounted; the lock stays outside.
    ///
    /// A browser that fails to start is started once more. Nothing has reached a
    /// provider yet, the first attempt is fully reaped and its reason logged, and a
    /// lasting fault still fails the second time.
    pub async fn start(
        &self,
        profile: &Path,
        mode: Mode,
        policy: NetworkPolicy,
    ) -> Result<Session> {
        match self.launch(profile, mode, policy).await {
            Err(error) if error.is(crate::Code::BrowserStartFailed) => {
                self.launch(profile, mode, policy).await
            }
            result => result,
        }
    }
    async fn launch(&self, profile: &Path, mode: Mode, policy: NetworkPolicy) -> Result<Session> {
        if matches!(mode, Mode::Windowed) {
            sandbox::trusted(Path::new("/usr/bin/Xvfb"), true)?;
        }
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new("browser_capacity_busy", 429))?;
        let lease = profile_lease(profile)?;
        scrub_reported(profile);
        // Credentials belong to the DSP vault; password-manager chrome must not
        // cover native PIN entry or persist another copy in the browser profile.
        let defaults = db::private_dir(&profile.join("Default"))?;
        let preferences = defaults.join("Preferences");
        db::private_file(&preferences, false)?;
        let mut settings: Value = if preferences.exists() {
            ensure(
                fs::metadata(&preferences)?.len() <= 4 * 1024 * 1024,
                "browser_profile_invalid",
                409,
            )?;
            serde_json::from_slice(&fs::read(&preferences)?)?
        } else {
            json!({})
        };
        ensure(
            settings.is_object()
                && (settings["profile"].is_null() || settings["profile"].is_object()),
            "browser_profile_invalid",
            409,
        )?;
        settings["credentials_enable_service"] = json!(false);
        settings["profile"]["password_manager_enabled"] = json!(false);
        db::write_private(&preferences, &serde_json::to_vec(&settings)?)?;
        let run = RunDirectory::create(&self.runs)?;
        let egress = Egress::start_with_policy(&run.0, policy)?;
        let mut child = sandbox::launch(self, &run.0, profile, mode)?;
        // What the sandbox itself prints, such as a namespace it could not create, arrives
        // before its worker can report anything.
        let sandbox_notes = worker::Notes::default();
        let drained = sandbox_notes.collect(child.stderr.take());
        let process_id = child.id().expect("new browser supervisor");
        let (sender, requests) = mpsc::channel(QUEUE_SIZE);
        let (stop, cancellation) = watch::channel(false);
        let (finished, closed) = watch::channel(None);
        let (ready, started) = oneshot::channel();
        let traced = profile.to_path_buf();
        tokio::spawn(async move {
            let mut child = child;
            serve_child(
                &mut child,
                requests,
                cancellation,
                ready,
                (sandbox_notes, drained),
            )
            .await;
            let exit = stop_child(&mut child).await;
            // Release leases only after reaping the namespace supervisor, and after the
            // pages it opened left nothing behind.
            drop(child);
            scrub_reported(&traced);
            drop(egress);
            drop(run);
            drop(lease);
            drop(slot);
            finished.send_replace(Some(exit));
        });
        let session = Session(Arc::new(Handle {
            sender,
            stop,
            closed,
            process_id,
        }));
        match started.await {
            Ok(Ok(())) => Ok(session),
            Ok(Err(error)) => {
                session.close().await;
                Err(error)
            }
            Err(_) => {
                session.close().await;
                Err(Error::new("browser_start_failed", 503))
            }
        }
    }
}

/// What a browser keeps of the pages it opened: its caches, history, sessions, icons,
/// page storage, and the stores its shopping and metrics features fill from what pages
/// show. Everything a collection reads goes into a database, so none of it may stay in
/// a profile: it is removed before every start and after every exit, leaving only what
/// signing in needs (cookies, local storage, preferences).
const PAGE_TRACES: &[&str] = &[
    "Default/Cache",
    "Default/Code Cache",
    "Default/GPUCache",
    "Default/DawnGraphiteCache",
    "Default/DawnWebGPUCache",
    "Default/History",
    "Default/History-journal",
    "Default/Visited Links",
    "Default/Top Sites",
    "Default/Top Sites-journal",
    "Default/Favicons",
    "Default/Favicons-journal",
    "Default/Shortcuts",
    "Default/Shortcuts-journal",
    "Default/Network Action Predictor",
    "Default/Network Action Predictor-journal",
    "Default/Sessions",
    "Default/Session Storage",
    "Default/Service Worker",
    "Default/blob_storage",
    "Default/Shared Dictionary",
    "Default/SharedStorage",
    "Default/SharedStorage-shm",
    "Default/SharedStorage-wal",
    "Default/Reporting and NEL",
    "Default/Reporting and NEL-journal",
    "Default/BrowsingTopicsSiteData",
    "Default/BrowsingTopicsSiteData-journal",
    "Default/BrowsingTopicsState",
    "Default/Site Characteristics Database",
    "Default/optimization_guide_hint_cache_store",
    "Default/parcel_tracking_db",
    "Default/chrome_cart_db",
    "Default/commerce_subscription_db",
    "Default/discount_infos_db",
    "Default/discounts_db",
    "Default/DIPS",
    "Default/DIPS-shm",
    "Default/DIPS-wal",
    "Default/DIPS-journal",
    "Default/Web Data",
    "Default/Web Data-journal",
    "Default/Account Web Data",
    "Default/Account Web Data-journal",
    "Default/Segmentation Platform",
    "Default/shared_proto_db",
    "segmentation_platform",
    "GPUPersistentCache",
    "BrowserMetrics",
    "BrowserMetrics-spare.pma",
    "CrashpadMetrics-active.pma",
    "config/browser-os/Crash Reports",
];
/// Removes every page trace from `profile`. Runs only while this process holds the
/// profile's lease, so no browser is writing to it. A trace that cannot be removed does
/// not keep the rest: every entry is tried, and the first failure is returned.
pub(crate) fn scrub(profile: &Path) -> Result<()> {
    let mut first = None;
    for entry in PAGE_TRACES {
        let result = remove_trace(profile, Path::new(entry));
        match result {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                first.get_or_insert(error);
            }
            _ => (),
        }
    }
    first.map_or(Ok(()), |error| Err(error.into()))
}
/// Refuse to traverse a link (or a file standing in for a directory) anywhere
/// before the trace itself. The final component may be a link: unlinking that
/// component removes the profile entry without touching its target.
fn remove_trace(profile: &Path, entry: &Path) -> std::io::Result<()> {
    let mut components = Vec::new();
    for component in entry.components() {
        match component {
            Component::Normal(name) => components.push(name),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "browser trace path is not relative",
                ));
            }
        }
    }
    let Some((final_name, ancestors)) = components.split_last() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "browser trace path is empty",
        ));
    };
    let mut path = profile.to_path_buf();
    for component in ancestors {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "browser trace ancestor is not a directory",
            ));
        }
        path.push(component);
    }
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "browser trace parent is not a directory",
        ));
    }
    path.push(final_name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(path),
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) => Err(error),
    }
}
/// A scrub that failed is reported, never fatal: the browser still starts or stops, and
/// the next start or exit tries again.
fn scrub_reported(profile: &Path) {
    if let Err(error) = scrub(profile) {
        crate::observability::event(
            "error",
            "browser.scrub_failed",
            serde_json::json!({"error":error.code}),
        );
    }
}

fn profile_lease(profile: &Path) -> Result<File> {
    db::private_dir(profile)?;
    let parent = profile
        .parent()
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    db::private_dir(parent)?;
    let name = profile
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    let path = parent.join(format!(".{name}.browseros.lock"));
    db::private_file(&path, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    let stat = file.metadata()?;
    ensure(
        stat.is_file()
            && stat.nlink() == 1
            && stat.uid() == unsafe { libc::geteuid() }
            && stat.mode() & 0o077 == 0,
        "unsafe_storage_file",
        500,
    )?;
    file.try_lock_exclusive()
        .map_err(|_| Error::new("browser_profile_busy", 409))?;
    Ok(file)
}

struct RunDirectory(PathBuf);
impl RunDirectory {
    fn create(root: &Path) -> Result<Self> {
        let path = root.join(crypto::id("browseros")?);
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }
}
impl Drop for RunDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Exit {
    pub graceful: bool,
    pub supervisor_reaped: bool,
}
struct Handle {
    sender: mpsc::Sender<Request>,
    stop: watch::Sender<bool>,
    closed: watch::Receiver<Option<Exit>>,
    process_id: u32,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.send_replace(true);
    }
}
#[derive(Clone)]
pub struct Session(Arc<Handle>);
struct Request {
    bytes: Vec<u8>,
    deadline: Instant,
    reply: oneshot::Sender<Result<Value>>,
}

impl Session {
    /// Host supervisor PID for operational diagnostics, never a control endpoint.
    pub fn process_id(&self) -> u32 {
        self.0.process_id
    }
    /// Serialized commands for trusted provider scripts. Queueing is bounded;
    /// abandoning an in-flight command retires this entire session.
    pub async fn command(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<Value> {
        ensure(
            !*self.0.stop.borrow() && self.0.closed.borrow().is_none(),
            "browser_closed",
            409,
        )?;
        self.send(WireCommand::Cdp {
            method: method.into(),
            params,
            session: session.map(str::to_owned),
        })
        .await
    }
    async fn send(&self, command: WireCommand) -> Result<Value> {
        let bytes = frame(&command, COMMAND_BYTES)?;
        let (reply, response) = oneshot::channel();
        self.0
            .sender
            .try_send(Request {
                bytes,
                deadline: Instant::now() + COMMAND_TIMEOUT,
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::new("browser_queue_full", 429),
                mpsc::error::TrySendError::Closed(_) => Error::new("browser_closed", 409),
            })?;
        response
            .await
            .map_err(|_| Error::new("browser_closed", 409))?
    }
    pub async fn navigation(&self, session: &str, previous: &str) -> Result<Value> {
        self.send(WireCommand::Navigation {
            session: session.into(),
            previous: previous.into(),
        })
        .await
    }
    pub async fn loading(&self, session: &str, loader: &str) -> Result<Value> {
        self.send(WireCommand::Loading {
            session: session.into(),
            loader: loader.into(),
        })
        .await
    }
    pub async fn event(&self, session: &str) -> Result<Value> {
        self.send(WireCommand::Event {
            session: session.into(),
        })
        .await
    }
    pub async fn native_move(&self, x: i32, y: i32) -> Result<Value> {
        self.send(WireCommand::NativeMove { x, y }).await
    }
    pub async fn native_click(&self, x: i32, y: i32) -> Result<Value> {
        self.send(WireCommand::NativeClick { x, y }).await
    }
    pub async fn native_type(&self, text: &str) -> Result<Value> {
        self.send(WireCommand::NativeType { text: text.into() })
            .await
    }
    pub async fn evaluate(&self, session: &str, expression: &str) -> Result<Value> {
        let result = self
            .command(
                "Runtime.evaluate",
                json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
                Some(session),
            )
            .await?;
        ensure(
            result.get("exceptionDetails").is_none(),
            "browser_script_failed",
            502,
        )?;
        Ok(result["result"]["value"].clone())
    }
    pub async fn close(&self) -> Exit {
        self.0.stop.send_replace(true);
        self.wait_closed().await
    }
    /// Wait for automatic cancellation, failure, expiry, or explicit close.
    pub async fn wait_closed(&self) -> Exit {
        let mut closed = self.0.closed.clone();
        loop {
            if let Some(exit) = *closed.borrow_and_update() {
                return exit;
            }
            if closed.changed().await.is_err() {
                return Exit {
                    graceful: false,
                    supervisor_reaped: false,
                };
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum WireCommand {
    Loading {
        session: String,
        loader: String,
    },
    Navigation {
        session: String,
        previous: String,
    },
    Event {
        session: String,
    },
    NativeMove {
        x: i32,
        y: i32,
    },
    NativeClick {
        x: i32,
        y: i32,
    },
    NativeType {
        text: String,
    },
    Cdp {
        method: String,
        params: Value,
        session: Option<String>,
    },
    Close,
}
fn frame(value: &impl Serialize, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure(bytes.len() < limit as usize, "browser_frame_too_large", 413)?;
    bytes.push(b'\n');
    Ok(bytes)
}
async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin), limit: u64) -> Result<Option<Value>> {
    let mut bytes = Vec::new();
    let count = reader.take(limit).read_until(b'\n', &mut bytes).await?;
    if count == 0 {
        return Ok(None);
    }
    ensure(bytes.last() == Some(&b'\n'), "browser_protocol_failed", 503)?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}
async fn cancelled(receiver: &mut watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() || receiver.changed().await.is_err() {
            return;
        }
    }
}
/// Why a worker never reported itself ready. The supervisor discards the worker's
/// standard error, so its own failure code arrives as a frame on this pipe.
fn start_failure(frame: Option<&Value>, sandbox: &str) -> String {
    let Some(frame) = frame else {
        return if sandbox.is_empty() {
            "worker exited before reporting".into()
        } else {
            // The worker never ran or died before its report; the sandbox said why.
            format!("worker exited before reporting: {sandbox}")
        };
    };
    match frame["error"].as_str() {
        Some(code) if !code.is_empty() => match frame["detail"].as_str() {
            // What the browser itself printed before failing, which nothing else keeps.
            Some(detail) if !detail.is_empty() => format!("{code}: {detail}"),
            _ => code.into(),
        },
        _ => format!("unexpected frame {frame}")
            .chars()
            .take(200)
            .collect(),
    }
}
fn start_failed(reason: String) -> Error {
    crate::observability::event("error", "browser.start_failed", json!({"reason":reason}));
    Error::caused("browser_start_failed", 503, reason)
}
async fn serve_child(
    child: &mut Child,
    mut requests: mpsc::Receiver<Request>,
    mut cancellation: watch::Receiver<bool>,
    mut ready: oneshot::Sender<Result<()>>,
    (sandbox, drained): (worker::Notes, Option<tokio::task::JoinHandle<()>>),
) {
    let Some(stdout) = child.stdout.take() else {
        let _ = ready.send(Err(start_failed("worker has no output pipe".into())));
        return;
    };
    // Child::wait closes child.stdin before waiting. Own it separately so exit
    // monitoring cannot signal EOF during an otherwise healthy idle session.
    let Some(mut input) = child.stdin.take() else {
        let _ = ready.send(Err(start_failed("worker has no input pipe".into())));
        return;
    };
    let mut reader = BufReader::new(stdout);
    let startup = tokio::select! {
        biased;
        _ = cancelled(&mut cancellation) => return,
        _ = ready.closed() => return,
        result = timeout(START_TIMEOUT, read_frame(&mut reader, RESPONSE_BYTES)) => result,
    };
    if !matches!(startup, Ok(Ok(Some(ref value))) if value["ready"] == true) {
        // The sandbox's output closes when it exits; give it a moment to be read in full.
        if matches!(startup, Ok(Ok(None)))
            && let Some(drained) = drained
        {
            let _ = timeout(Duration::from_secs(1), drained).await;
        }
        let reason = match &startup {
            Err(_) => format!("no report within {}s", START_TIMEOUT.as_secs()),
            Ok(Err(error)) => format!("unreadable report: {}", error.code),
            Ok(Ok(frame)) => start_failure(frame.as_ref(), &sandbox.tail()),
        };
        let _ = ready.send(Err(start_failed(reason)));
        return;
    }
    if ready.send(Ok(())).is_err() {
        return;
    }
    let lifetime = Instant::now() + Duration::from_secs(1800);
    loop {
        let request = tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => break,
            _ = tokio::time::sleep_until(lifetime) => break,
            _ = tokio::time::sleep(Duration::from_secs(600)) => break,
            _ = child.wait() => break,
            request = requests.recv() => request,
        };
        let Some(mut request) = request else {
            break;
        };
        if request.reply.is_closed() {
            continue;
        }
        if Instant::now() >= request.deadline {
            let _ = request
                .reply
                .send(Err(Error::new("browser_command_timeout", 504)));
            continue;
        }
        let operation = async {
            input.write_all(&request.bytes).await?;
            let response = read_frame(&mut reader, RESPONSE_BYTES)
                .await?
                .ok_or_else(|| Error::new("browser_lost", 503))?;
            ensure(
                response.get("result").is_some() && response.get("error").is_none(),
                "browser_command_failed",
                502,
            )?;
            Ok(response["result"].clone())
        };
        let result = tokio::select! {
            biased;
            _ = cancelled(&mut cancellation) => break,
            _ = request.reply.closed() => break,
            result = timeout_at(request.deadline.min(lifetime),
                operation) => result.unwrap_or_else(|_| Err(Error::new("browser_command_timeout", 504))),
        };
        let failed = result.is_err();
        let _ = request.reply.send(result);
        if failed {
            break;
        }
    }
}
async fn stop_child(child: &mut Child) -> Exit {
    if let Some(mut input) = child.stdin.take() {
        let _ = timeout(
            Duration::from_millis(100),
            input.write_all(b"{\"action\":\"close\"}\n"),
        )
        .await;
    }
    if let Ok(Ok(status)) = timeout(Duration::from_secs(3), child.wait()).await {
        return Exit {
            graceful: status.success(),
            supervisor_reaped: true,
        };
    }
    let _ = child.start_kill();
    Exit {
        graceful: false,
        supervisor_reaped: child.wait().await.is_ok(),
    }
}

/// Hidden process entrypoint. Must run before Config::load or any database access.
pub async fn worker_main(mode: &str) -> Result<()> {
    worker::run(mode).await
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn a_scrub_removes_every_page_trace_and_keeps_what_signing_in_needs() {
        use std::os::unix::fs::PermissionsExt;
        let profile = tempfile::tempdir().unwrap();
        let default = profile.path().join("Default");
        fs::create_dir_all(default.join("Cache/Cache_Data")).unwrap();
        fs::create_dir_all(default.join("Service Worker/CacheStorage")).unwrap();
        for file in ["History", "Web Data", "Cookies", "Preferences"] {
            fs::write(default.join(file), b"x").unwrap();
        }
        fs::create_dir_all(default.join("Local Storage")).unwrap();
        // A trace it cannot remove is reported, and every other trace still goes.
        let locked = default.join("Service Worker/CacheStorage");
        fs::write(locked.join("entry"), b"x").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        assert!(scrub(profile.path()).is_err());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        for gone in ["Cache", "History", "Web Data"] {
            assert!(!default.join(gone).exists(), "{gone}");
        }
        for kept in ["Cookies", "Preferences", "Local Storage"] {
            assert!(default.join(kept).exists(), "{kept}");
        }
        assert!(scrub(profile.path()).is_ok());
        assert!(!default.join("Service Worker").exists());
    }
    #[test]
    fn a_scrub_never_follows_ancestor_links_outside_the_profile() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let outside = root.path().join("outside");
        fs::create_dir_all(&profile).unwrap();
        fs::create_dir_all(outside.join("Cache")).unwrap();
        fs::write(outside.join("Cache/sentinel"), b"outside").unwrap();
        symlink(&outside, profile.join("Default")).unwrap();
        fs::write(profile.join("BrowserMetrics"), b"inside").unwrap();

        assert!(scrub(&profile).is_err());
        assert_eq!(
            fs::read(outside.join("Cache/sentinel")).unwrap(),
            b"outside"
        );
        assert!(
            fs::symlink_metadata(profile.join("Default"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!profile.join("BrowserMetrics").exists());
    }
    #[test]
    fn a_scrub_rejects_relative_dangling_and_nested_ancestor_links() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let outside = root.path().join("outside");
        fs::create_dir_all(profile.join("Default")).unwrap();
        fs::create_dir_all(profile.join("config")).unwrap();
        fs::create_dir_all(outside.join("Crash Reports")).unwrap();
        fs::write(outside.join("Crash Reports/sentinel"), b"outside").unwrap();
        symlink("../../outside", profile.join("config/browser-os")).unwrap();

        assert!(scrub(&profile).is_err());
        assert_eq!(
            fs::read(outside.join("Crash Reports/sentinel")).unwrap(),
            b"outside"
        );
        fs::remove_file(profile.join("config/browser-os")).unwrap();
        symlink(
            root.path().join("missing"),
            profile.join("config/browser-os"),
        )
        .unwrap();
        assert!(scrub(&profile).is_err());
        assert!(
            fs::symlink_metadata(profile.join("config/browser-os"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
    #[test]
    fn a_scrub_unlinks_a_final_link_without_touching_its_target() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        let outside = root.path().join("outside");
        fs::create_dir_all(profile.join("Default")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("history"), b"outside").unwrap();
        symlink(outside.join("history"), profile.join("Default/History")).unwrap();

        assert!(scrub(&profile).is_ok());
        assert!(!profile.join("Default/History").exists());
        assert_eq!(fs::read(outside.join("history")).unwrap(), b"outside");
    }
    #[test]
    fn a_scrub_refuses_non_directory_ancestors_and_a_linked_profile_root() {
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("Default"), b"not a directory").unwrap();
        fs::write(profile.join("BrowserMetrics"), b"safe trace").unwrap();

        assert!(scrub(&profile).is_err());
        assert_eq!(
            fs::read(profile.join("Default")).unwrap(),
            b"not a directory"
        );
        assert!(!profile.join("BrowserMetrics").exists());

        fs::remove_file(profile.join("Default")).unwrap();
        fs::write(profile.join("BrowserMetrics"), b"outside").unwrap();
        let linked = root.path().join("linked-profile");
        symlink(&profile, &linked).unwrap();
        assert!(scrub(&linked).is_err());
        assert_eq!(
            fs::read(profile.join("BrowserMetrics")).unwrap(),
            b"outside"
        );
    }
    #[test]
    fn a_failed_start_names_the_workers_own_reason() {
        assert_eq!(
            start_failure(Some(&json!({"error":"browser_display_failed"})), ""),
            "browser_display_failed"
        );
        assert_eq!(start_failure(None, ""), "worker exited before reporting");
        // When the worker never reports, the sandbox's own complaint explains why.
        assert_eq!(
            start_failure(None, "bwrap: setting up uid map: Permission denied"),
            "worker exited before reporting: bwrap: setting up uid map: Permission denied"
        );
        // A worker that did report is the authority; the sandbox's echo of it adds nothing.
        assert_eq!(
            start_failure(
                Some(&json!({"error":"browser_lost"})),
                "core.failed browser_lost"
            ),
            "browser_lost"
        );
        // An unexpected frame is kept, bounded, rather than replaced by a generic failure.
        assert_eq!(
            start_failure(Some(&json!({"ready":false})), ""),
            "unexpected frame {\"ready\":false}"
        );
        assert_eq!(
            start_failure(
                Some(&json!({"error":"browser_lost","detail":"bwrap: no permission"})),
                ""
            ),
            "browser_lost: bwrap: no permission"
        );
        assert_eq!(
            start_failure(Some(&json!({"error":""})), ""),
            "unexpected frame {\"error\":\"\"}"
        );
        assert_eq!(
            start_failure(Some(&json!({"error":"x".repeat(400)})), "").len(),
            400
        );
        assert!(start_failure(Some(&json!({"note":"y".repeat(400)})), "").len() <= 200);
    }
}
