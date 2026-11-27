//! A live reader's browsers close as it stops reading, each on its own: one whose worker is
//! busy, as while its browser still starts, is closed to its hook at once and holds up no other
//! DSP's close. A job reading with a saved session that finds no browser free for a page lets
//! its session go, and one that lost its authority as a page was needed starts no browser.
use super::*;
use crate::testing;

const READER: &str = "probe.routes";

/// A reader's browser for `dsp`, registered as `Reading::browser` registers one.
fn opened(state: &State, dsp: &str) -> Arc<Session> {
    registered(state, dsp, Some(READER), true)
}
/// A ready session for `dsp`, opened by `reader` or else a job, registered as a start
/// registers one.
pub(super) fn registered(
    state: &State,
    dsp: &str,
    reader: Option<&'static str>,
    browserless: bool,
) -> Arc<Session> {
    let provider = Provider::new("probe_site");
    let (cancel, _) = watch::channel(false);
    let session = Arc::new(Session {
        id: format!("run_{dsp}"),
        dsp: dsp.into(),
        provider,
        revision: 1,
        timezone: "America/Chicago".into(),
        run: state
            .config
            .environment_root()
            .join("browser-runs")
            .join(dsp),
        profile: std::path::PathBuf::new(),
        process_id: std::sync::atomic::AtomicU32::new(0),
        observed_pss: std::sync::atomic::AtomicU64::new(0),
        browser_exited: std::sync::atomic::AtomicBool::new(false),
        status: AtomicU8::new(1),
        worker: AsyncMutex::new(None),
        commands: tokio::sync::Semaphore::new(32),
        cancel,
        browserless: AtomicBool::new(browserless),
        saved: Mutex::new(None),
        started: std::time::Instant::now(),
        last_used: Mutex::new(std::time::Instant::now()),
        collecting: std::sync::atomic::AtomicBool::new(true),
        reader,
        credential_lease: Mutex::new(Some(tempfile::tempfile().unwrap())),
    });
    let mut sessions = state.browsers.sessions.lock().unwrap();
    sessions.insert(provider.key(dsp), session.clone());
    session
}
fn leased(session: &Session) -> bool {
    session.credential_lease.lock().unwrap().is_some()
}

#[cfg_attr(target_os = "macos", ignore = "paused clock races I/O; CI runs it")]
#[tokio::test(start_paused = true)]
async fn a_readers_browser_still_starting_holds_up_no_other_dsps_close() {
    testing::install(&[], &[]);
    let (_root, store) = testing::store();
    let state = State::new(store.config.clone()).unwrap();
    let (starting, other) = (opened(&state, "dsp_starting"), opened(&state, "dsp_other"));
    // Its worker is busy, as while its browser still starts.
    let worker = starting.worker.lock().await;
    state.close_reader_browsers("dsp_starting", READER);
    state.close_reader_browsers("dsp_other", READER);
    tokio::time::sleep(Duration::from_millis(1)).await;
    // The other has closed whole; the first is closed to its hook, and gone from the server's
    // browsers, though it lets go of its sign-in only once its worker is free.
    assert!(other.closed() && !leased(&other));
    assert!(starting.closed() && leased(&starting));
    assert_eq!(state.browsers.active(), 0);
    drop(worker);
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert!(!leased(&starting));
}

#[tokio::test]
async fn a_saved_sessions_job_finding_no_browser_free_for_a_page_lets_its_session_go() {
    testing::install(&[], &[]);
    let (_root, store) = testing::store();
    let state = State::new(store.config.clone()).unwrap();
    // Other DSPs' jobs hold every browser the server runs.
    for n in 0..state.config.browser_capacity {
        registered(&state, &format!("dsp_busy_{n}"), None, false);
    }
    let job = registered(&state, "dsp_saved", None, true);
    job.became(SavedSession::Browserless);
    let authority = ProviderAuthority::Job {
        id: "job_saved".into(),
        owner: "worker".into(),
    };
    let refused = state.browser_for(&job, &authority).await.unwrap_err();
    // The job goes back to wait for a browser: its next start finds no session left busy.
    assert!(state.browsers.get_for("dsp_saved", job.provider).is_none());
    assert!(job.closed());
    assert_eq!(refused.code, "browser_capacity_busy");
}

/// A driver reading with a saved session whose collection needs a page.
struct NeedsPage;
impl Driver for NeedsPage {
    fn request(&mut self, _: Value) -> Pending<'_, Value> {
        Box::pin(async { Err(Error::new(crate::Code::BrowserRequired.as_str(), 409)) })
    }
    fn collect<'a>(&'a mut self, _: &'a Run<'a>) -> Pending<'a, Collected> {
        Box::pin(async { Err(Error::new(crate::Code::BrowserRequired.as_str(), 409)) })
    }
    fn browser(&self) -> Option<&browseros::Session> {
        None
    }
}

#[tokio::test]
async fn a_revoked_job_needing_a_page_starts_no_browser() {
    testing::install(&[], &[]);
    let (_root, store) = testing::store();
    let state = State::new(store.config.clone()).unwrap();
    let job = registered(&state, "dsp_saved", None, true);
    job.became(SavedSession::Browserless);
    job.collecting.store(false, Ordering::SeqCst);
    *job.worker.lock().await = Some(Box::new(NeedsPage));
    let metrics = crate::collection::metrics::Recorder::new(&json!({}));
    // A job no worker holds, as one cancelled while its collection read.
    let refused = job
        .collect(&state, "job_saved", "worker", &metrics, &json!({}), 1)
        .await;
    assert_eq!(
        refused.err().map(|error| error.code).as_deref(),
        Some("job_not_found")
    );
    assert!(job.browserless());
    assert_eq!(job.saved_session(), Some(SavedSession::Browserless));
}
