//! What Schedules' backend tests collect: the week Cortex's fixture collects for the DSP.
use crate::backend::storage::bind_week_request;
use dispatch_core::{db::Store, manifest::Collector};
use dispatch_cortex::shifts::Capture;
use serde_json::json;

/// The DSP's station and abbreviation, which a week's request needs.
pub(crate) fn station(store: &Store, dsp: &str) {
    store
        .set_profile(dsp, json!({"abbreviation":"FXTR","stationCode":"TST1"}))
        .unwrap();
}
/// The week Cortex's fixture collects for the DSP, as collected at `when`.
pub(crate) fn week(store: &Store, dsp: &str, when: i64) -> Capture {
    let request = bind_week_request(store, dsp).unwrap();
    let collected = dispatch_cortex::COLLECTOR
        .fixture("America/Chicago", &serde_json::to_value(request).unwrap())
        .unwrap();
    let mut capture: Capture = serde_json::from_value(collected.data).unwrap();
    (capture.started_at, capture.finished_at) = (when, when);
    capture
}
