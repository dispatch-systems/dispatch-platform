//! What reaches a phone as it happens. A phone waits on its outbox, a long poll, which answers once
//! Dispatch has a text for it, or twenty seconds pass. Filing hears of the texts the phone side
//! keeps from whoever kept them, once they're kept (`texts::filing::told`).
use dispatch_core::{State, server::live::Topic};

/// The phones' live channel: a DSP's phones waiting on their outbox wake when it changes. Apart
/// from the page's (`backend::live`), as their waits serve different readers.
pub const TOPIC: Topic = Topic("phones");

/// Wakes the DSP's phones waiting on their outbox, as a text is queued for one, or one is removed.
pub fn queued(state: &State, dsp: &str) {
    state.topics.notify(TOPIC, dsp);
}
