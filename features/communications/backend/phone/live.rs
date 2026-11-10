//! What reaches a phone as it happens. A phone waits on its outbox, a long poll, which answers once
//! Dispatch has a text for it, or twenty seconds pass. The page hears of the texts the phone side
//! keeps from filing, once it's filed them (`texts::filing::changing`).
use dispatch_core::{
    State,
    server::live::{ScopedListener, Topic},
};

/// The phones' live channel: phones wait by device; lifecycle changes wake the DSP. Apart
/// from the page's (`backend::live`), as their waits serve different readers.
pub const TOPIC: Topic = Topic("phones");

/// Wakes every phone in the DSP for lifecycle changes, or when the exact device is unknown.
pub fn queued(state: &State, dsp: &str) {
    state.topics.notify(TOPIC, dsp);
}

/// Wakes the phone selected by the committed write, without choosing again after it.
pub fn queued_for(state: &State, dsp: &str, device: Option<&str>) {
    if let Some(device) = device {
        state.topics.notify_scoped(TOPIC, dsp, device);
    } else {
        queued(state, dsp);
    }
}

/// A phone's wake source. A closed channel causes one fresh authority check, then stays quiet
/// until the heartbeat or admission wakes the request, rather than spinning on closure.
pub struct Listener {
    inner: ScopedListener,
    closed: bool,
}
impl Listener {
    pub async fn changed(&mut self) {
        if self.closed {
            std::future::pending::<()>().await;
        }
        if self.inner.changed().await.is_err() {
            self.closed = true;
        }
    }
}

/// Subscribe before the first outbox read, including while waiting for idle admission.
pub fn listen(state: &State, dsp: &str, device: &str) -> Listener {
    Listener {
        inner: state.topics.subscribe_scoped(TOPIC, dsp, device),
        closed: false,
    }
}
