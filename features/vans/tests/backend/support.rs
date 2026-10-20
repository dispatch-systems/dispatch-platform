//! What Vans' backend tests collect: the fleet Cortex's fixture collects for the DSP.
use crate::backend::storage::bind_fleet_request;
use dispatch_core::{db::Store, manifest::Collector};
use dispatch_cortex::vans::Capture;

/// The fleet Cortex's fixture collects for the DSP, as collected at `when`.
pub(crate) fn fleet(store: &Store, dsp: &str, when: i64) -> Capture {
    let request = bind_fleet_request(store, dsp).unwrap();
    let collected = dispatch_cortex::COLLECTOR
        .fixture("America/Chicago", &request)
        .unwrap();
    let mut capture: Capture = serde_json::from_value(collected.data).unwrap();
    (capture.started_at, capture.finished_at) = (when, when);
    capture
}
