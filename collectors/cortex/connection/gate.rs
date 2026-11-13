//! One account's turn at Cortex's execution API, shared by everything on this server that
//! reads a day's routes from it: Amazon limits each account, not each reader, so a live read
//! and a job that start together must not each ask for a burst of their own.
//!
//! A gate admits a few requests at once. A request Cortex throttles (HTTP 429) starts a
//! cooldown for every reader of the account and narrows what is admitted next, down to one
//! at a time; reads that then meet no throttle widen it again, a step at a time. The gates
//! live as long as the server, one for each company at each Cortex origin.
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, MutexGuard},
    time::Duration,
};
use tokio::{sync::Notify, time::Instant};

/// Requests admitted at once: four, the widest Cortex was seen to answer without throttling
/// since it began to, then fewer after each round it throttled.
const WIDTHS: [usize; 3] = [4, 2, 1];
/// Reads in a row that meet no throttle before a narrowed gate widens a step.
const CLEAN_READS: usize = 3;

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
    /// A place among the requests admitted, once one is free at the width it has then, and
    /// whether it was waited for: what its caller checked before may have changed since.
    pub(crate) async fn admit(self: &Arc<Self>) -> (Admitted, bool) {
        let mut waited = false;
        loop {
            let freed = self.freed.notified();
            tokio::pin!(freed);
            // Listening before the count is read, so a place freed meanwhile is not missed.
            freed.as_mut().enable();
            {
                let mut state = self.state();
                if state.admitted < WIDTHS[state.narrowed] {
                    state.admitted += 1;
                    return (Admitted(self.clone()), waited);
                }
            }
            waited = true;
            freed.await;
        }
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
