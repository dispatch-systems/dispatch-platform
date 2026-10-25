//! A live reader's browsers close as it stops reading, each on its own: one whose worker is
//! busy, as while its browser still starts, is closed to its hook at once and holds up no other
//! DSP's close.
use super::*;
use crate::testing;

const READER: &str = "probe.routes";

/// A reader's browser for `dsp`, registered as `Reading::browser` registers one.
fn opened(state: &State, dsp: &str) -> Arc<Session> {
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
        process_id: std::sync::atomic::AtomicU32::new(0),
        observed_pss: std::sync::atomic::AtomicU64::new(0),
        browser_exited: std::sync::atomic::AtomicBool::new(false),
        status: AtomicU8::new(1),
        worker: AsyncMutex::new(None),
        commands: tokio::sync::Semaphore::new(32),
        cancel,
        fixture: true,
        started: std::time::Instant::now(),
        last_used: Mutex::new(std::time::Instant::now()),
        collecting: std::sync::atomic::AtomicBool::new(true),
        reader: Some(READER),
        credential_lease: Mutex::new(Some(tempfile::tempfile().unwrap())),
    });
    let mut sessions = state.browsers.sessions.lock().unwrap();
    sessions.insert(provider.key(dsp), session.clone());
    session
}
fn leased(session: &Session) -> bool {
    session.credential_lease.lock().unwrap().is_some()
}

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
