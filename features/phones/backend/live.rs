//! What reaches a phone, and every feature reading texts, as it happens. A phone waits on its
//! outbox, a long poll, which answers once Dispatch has a text for it, or twenty seconds pass;
//! a feature hears `TextsChanged` once texts or how they went are kept, and reads on from where
//! it left off.
use dispatch_core::{State, manifest::events, server::live::Topic};
use dispatch_phones_contract::TextsChanged;
use std::sync::Arc;

/// Phones' live channel: a DSP's phones waiting on their outbox wake when it changes.
pub const TOPIC: Topic = Topic("phones");

/// Wakes the DSP's phones waiting on their outbox, as a text is queued for one.
pub fn queued(state: &State, dsp: &str) {
    state.topics.notify(TOPIC, dsp);
}
/// Tells every feature reading the DSP's texts that some changed.
pub async fn changed(state: &Arc<State>, dsp: &str) {
    events::publish(state, dsp, TextsChanged).await;
}
