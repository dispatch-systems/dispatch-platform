//! One account's turn at Cortex, shared by everything on this server that asks it for an
//! account's data: a day's routes and meals, the performance datasets, DVIC's weeks and the
//! roster's photos. Amazon limits each account, not each reader, so a live read and a job
//! that start together must not each ask for a burst of their own.
//!
//! A gate admits a few requests at once. A request Cortex throttles (HTTP 429) starts a
//! cooldown for every reader of the account and narrows what is admitted next, down to one
//! at a time; reads that then meet no throttle widen it again, a step at a time. The gates
//! live as long as the server, one for each company at each Cortex origin.
use crate as cortex;
use dispatch_core::{
    Error, Result,
    collection::browser::{Pending, Run, http::WireError},
    ensure,
    foundation::observability,
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        Arc, LazyLock, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::Notify,
    time::{Instant, sleep_until},
};

/// Requests admitted at once: four, the widest Cortex was seen to answer without throttling
/// since it began to, then fewer after each round it throttled.
const WIDTHS: [usize; 3] = [4, 2, 1];
/// How long a throttle that names no delay leaves the account alone.
pub(crate) const PAUSE: Duration = Duration::from_secs(10);
/// Reads in a row that meet no throttle before a narrowed gate widens a step.
const CLEAN_READS: usize = 3;
/// Times a turn's request is asked for at most, so one Cortex throttles with no delay ends.
const ROUNDS: usize = 12;
/// What a turn spends waiting on throttling at most, as a day's routes do: a longer
/// cooldown fails promptly and stays with the gate for a later attempt.
const BUDGET: Duration = Duration::from_secs(120);
/// A throttle's delay, bounded before it is added to an instant.
const MAX_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);

/// Each gate by its account: the origin, then the company there.
type Gates = HashMap<(String, String), Arc<Gate>>;
static GATES: LazyLock<Mutex<Gates>> = LazyLock::new(Mutex::default);

/// The gate of `provider`, a company on Cortex, at `origin`.
pub(crate) fn of(origin: &str, provider: &str) -> Arc<Gate> {
    GATES
        .lock()
        .expect("cortex gates")
        .entry((origin.to_owned(), provider.to_owned()))
        .or_default()
        .clone()
}

#[derive(Default)]
pub(crate) struct Gate {
    /// Changed a step at a time, each whole: two readers of the account never widen it twice
    /// for one run of clean reads, nor write over each other's throttle.
    state: Mutex<State>,
    freed: Notify,
}
#[derive(Default)]
struct State {
    /// Until when no request is admitted, after a throttle.
    until: Option<Instant>,
    /// How far down `WIDTHS` throttled rounds have brought the account's reads.
    narrowed: usize,
    /// Cortex throttled since itineraries were last asked for: the next round narrows. Kept
    /// here, it outlasts a read that ends on a long cooldown or is cancelled in one.
    narrows: bool,
    /// Reads in a row that met no throttle, while narrowed.
    clean: usize,
    /// Requests admitted and not yet answered.
    admitted: usize,
}

/// A request's place among those admitted, given up as it is dropped.
pub(crate) struct Admitted(Arc<Gate>);
impl Drop for Admitted {
    fn drop(&mut self) {
        self.0.state().admitted -= 1;
        self.0.freed.notify_waiters();
    }
}

impl Gate {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("cortex gate")
    }
    /// Requests admitted at once now.
    pub(crate) fn width(&self) -> usize {
        WIDTHS[self.state().narrowed]
    }
    /// Whether it admits fewer at once than it starts at.
    pub(crate) fn narrowed(&self) -> bool {
        self.state().narrowed > 0
    }
    /// A place among the requests admitted, once one is free at the width it has then.
    pub(crate) async fn admit(self: &Arc<Self>) -> Admitted {
        loop {
            let freed = self.freed.notified();
            tokio::pin!(freed);
            // Listening before the count is read, so a place freed meanwhile is not missed.
            freed.as_mut().enable();
            {
                let mut state = self.state();
                if state.admitted < WIDTHS[state.narrowed] {
                    state.admitted += 1;
                    return Admitted(self.clone());
                }
            }
            freed.await;
        }
    }
    /// A place among the requests admitted, when one is free now and no throttle's cooldown
    /// is under way or yet to be waited out.
    pub(crate) fn admit_now(self: &Arc<Self>) -> Option<Admitted> {
        let mut state = self.state();
        (state.until.is_none() && state.admitted < WIDTHS[state.narrowed]).then(|| {
            state.admitted += 1;
            Admitted(self.clone())
        })
    }
    /// Before a round of requests: one that follows a throttle admits fewer at once, down to
    /// one.
    pub(crate) fn narrow(&self) {
        let mut state = self.state();
        if std::mem::take(&mut state.narrows) {
            state.narrowed = (state.narrowed + 1).min(WIDTHS.len() - 1);
            state.clean = 0;
        }
    }
    /// Cortex throttled a request: no reader of the account asks again before `retry` has
    /// passed, nor before a longer cooldown already under way.
    pub(crate) fn throttled(&self, retry: Duration) {
        let until = Instant::now() + retry;
        let mut state = self.state();
        state.narrows = true;
        state.clean = 0;
        state.until = Some(state.until.map_or(until, |previous| previous.max(until)));
    }
    /// A read ended whole having met no throttle: enough of them in a row widen a narrowed
    /// gate a step.
    pub(crate) fn read_clean(&self) {
        let mut state = self.state();
        if state.narrowed == 0 || state.narrows {
            return;
        }
        state.clean += 1;
        if state.clean >= CLEAN_READS {
            state.clean = 0;
            state.narrowed -= 1;
            drop(state);
            // The places a wider gate has are free at once.
            self.freed.notify_waiters();
        }
    }
    /// Whether a cooldown is under way.
    pub(crate) fn cooling(&self) -> bool {
        self.cooldown().is_some_and(|until| until > Instant::now())
    }
    /// The cooldown a throttle left, under way or passed unobserved since.
    pub(crate) fn cooldown(&self) -> Option<Instant> {
        self.state().until
    }
    /// The cooldown until `waited` was waited out: it is let go, unless a throttle since
    /// made it longer.
    pub(crate) fn cooled(&self, waited: Instant) {
        let mut state = self.state();
        if state.until == Some(waited) {
            state.until = None;
        }
    }
}

/// One read's requests at the account's gate, each asked on its own rather than in rounds,
/// as the performance datasets, DVIC's weeks and the roster's photos are. A request waits out
/// a cooldown under way, then its place; one Cortex throttles is asked again once the
/// account has cooled, fewer at once, while the read's budget for waiting and `ROUNDS` last.
/// A healthy read has no timer or added wait.
/// Whether what a turn reads for may still ask Cortex, asked just before each request is
/// sent, once it waited its place, as each of a day's route reads is.
pub(crate) type Authority = Box<dyn Fn() -> Pending<'static, ()> + Send + Sync>;
/// The authority of the job `run` is an attempt of: still this worker's, not cancelled, and
/// its DSP, feature, member and connection all as they were.
pub(crate) fn job(run: &Run<'_>) -> Authority {
    let (state, job, owner) = (run.state.clone(), run.job.to_owned(), run.owner.to_owned());
    Box::new(move || {
        let (state, job, owner) = (state.clone(), job.clone(), owner.clone());
        Box::pin(async move {
            state
                .read(move |db| db.guard(&job, &owner).map(|_| ()))
                .await
        })
    })
}
pub(crate) struct Turn {
    gate: Arc<Gate>,
    authority: Authority,
    waited: Mutex<Waited>,
    /// Whether it asked anything, and whether Cortex throttled any of it.
    asked: AtomicBool,
    throttled: AtomicBool,
    /// Whether what it reads for failed already, so that a request waiting its place is not
    /// sent.
    stopped: AtomicBool,
}
/// What a turn has waited on throttling: the cooldowns its requests waited out together
/// are spent once.
struct Waited {
    budget: Duration,
    until: Option<Instant>,
}
impl Turn {
    /// A read at the gate of `provider`, a company on Cortex, at `origin`, for as long as
    /// `authority` allows. One that follows a throttle admits fewer at once, as a round of a
    /// day's routes would.
    pub(crate) fn of(origin: &str, provider: &str, authority: Authority) -> Self {
        let gate = of(origin, provider);
        gate.narrow();
        Self {
            gate,
            authority,
            waited: Mutex::new(Waited {
                budget: BUDGET,
                until: None,
            }),
            asked: AtomicBool::new(false),
            throttled: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }
    }
    /// What it reads for has failed: nothing it has not yet sent is sent.
    pub(crate) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }
    /// What `send` answered once admitted: asked again after each throttle while the turn
    /// lasts. `endpoint`, fixed and without identifiers, is what a throttle is logged as.
    /// A throttle that outlasts the turn fails it (`cortex_throttled`), keeping its cooldown
    /// with the gate.
    pub(crate) async fn ask<T, Fut>(
        &self,
        endpoint: &'static str,
        send: impl Fn() -> Fut,
    ) -> Result<std::result::Result<T, WireError>>
    where
        Fut: Future<Output = std::result::Result<T, WireError>>,
    {
        self.asked.store(true, Ordering::Relaxed);
        for _ in 0..ROUNDS {
            let admitted = self.admitted().await?;
            // A request that waited may no longer be the reader's to send.
            ensure(!self.stopped.load(Ordering::SeqCst), STOPPED, 409)?;
            (self.authority)().await?;
            let answer = send().await;
            match answer {
                // The cooldown is the gate's before the place is free, so no request
                // admitted in its place goes out unaware of it.
                Err(error) if error.status == Some(429) => {
                    self.throttled(endpoint, &error);
                    drop(admitted);
                }
                answer => return Ok(answer),
            }
        }
        Err(throttled())
    }
    /// The read ended whole: one that met no throttle counts toward widening a narrowed gate.
    pub(crate) fn finished(&self) {
        if self.asked.load(Ordering::Relaxed) && !self.throttled.load(Ordering::Relaxed) {
            self.gate.read_clean();
        }
    }
    /// A place among the account's requests, once no cooldown is under way.
    async fn admitted(&self) -> Result<Admitted> {
        loop {
            self.cooled().await?;
            let admitted = self.gate.admit().await;
            // A throttle met while it waited for its place is waited out first.
            if self.gate.cooldown().is_none() {
                return Ok(admitted);
            }
        }
    }
    /// A cooldown under way, or passed unobserved, waited out within the turn's budget; the
    /// gate then admits fewer at once.
    async fn cooled(&self) -> Result<()> {
        ensure(!self.stopped.load(Ordering::SeqCst), STOPPED, 409)?;
        let Some(until) = self.gate.cooldown() else {
            return Ok(());
        };
        {
            let mut waited = self.waited.lock().expect("cortex turn");
            let from = waited
                .until
                .map_or(Instant::now(), |w| w.max(Instant::now()));
            let delay = until.saturating_duration_since(from);
            if delay > waited.budget {
                return Err(throttled());
            }
            waited.budget -= delay;
            waited.until = Some(waited.until.map_or(until, |w| w.max(until)));
        }
        sleep_until(until).await;
        self.gate.cooled(until);
        self.gate.narrow();
        Ok(())
    }
    fn throttled(&self, endpoint: &'static str, error: &WireError) {
        self.throttled.store(true, Ordering::Relaxed);
        self.gate
            .throttled(error.retry_after.unwrap_or(PAUSE).min(MAX_COOLDOWN));
        observability::event(
            "warn",
            "cortex.read_throttled",
            json!({"endpoint":endpoint,"downloads":self.gate.width(),
                "retryAfterMs":error.retry_after.map(|d|d.as_millis().min(u64::MAX as u128) as u64)}),
        );
    }
}
/// What a request a stopped turn did not send fails with: never a read's own error, which
/// the request that failed it gives.
const STOPPED: &str = "cortex_read_stopped";
/// Whether `error` is only a request a stopped turn did not send.
pub(crate) fn stopped(error: &Error) -> bool {
    error.code == STOPPED
}
fn throttled() -> Error {
    Error::new(cortex::codes::CORTEX_THROTTLED.as_str(), 502)
}
