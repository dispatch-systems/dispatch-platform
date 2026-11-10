//! Internal Rust browser runtime. Provider adapters own these handles; no raw CDP
//! endpoint, client-supplied path, or script is exposed through the platform API.
mod cdp;
mod loading;
mod native;
mod profile;
mod projection;
mod protocol;
mod sandbox;
mod worker;
pub use projection::VerificationAuthority;
/// Hidden process entrypoint. Must run before Config::load or any database access.
pub use worker::run as worker_main;

use super::egress::Egress;
use crate::{
    Error, Result, db, ensure,
    foundation::{config, crypto},
};
use protocol::{COMMAND_BYTES, RESPONSE_BYTES, WireCommand, frame, read_frame};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::Child,
    sync::{Semaphore, mpsc, oneshot, watch},
    time::{Instant, timeout, timeout_at},
};

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
    /// A provider's own hosts, as its collector declares them.
    Hosts(&'static super::egress::HostPolicy),
    /// Synthetic local server, reachable only as fixture.dispatch.invalid.
    Fixture(std::num::NonZeroU16),
}

pub struct Runtime {
    browser: PathBuf,
    sandbox: PathBuf,
    executable: PathBuf,
    runs: PathBuf,
    slots: Arc<Semaphore>,
    verification: Option<VerificationAuthority>,
}
impl Runtime {
    pub fn new(
        browser: &Path,
        sandbox: &Path,
        executable: &Path,
        runs: &Path,
        capacity: usize,
    ) -> Result<Self> {
        config::ensure_browser_capacity(capacity)?;
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
            verification: None,
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
            if let Some(authority) = &self.verification {
                authority.display()?;
            } else {
                sandbox::trusted(Path::new("/usr/bin/Xvfb"), true)?;
            }
        }
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new("browser_capacity_busy", 429))?;
        let lease = profile::lease(profile)?;
        profile::scrub_reported(profile);
        profile::prepare(profile)?;
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
            profile::scrub_reported(&traced);
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
    crate::foundation::observability::event(
        "error",
        "browser.start_failed",
        json!({"reason":reason}),
    );
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

#[cfg(test)]
#[path = "../../../tests/backend/browser/browseros/mod.rs"]
mod tests;
