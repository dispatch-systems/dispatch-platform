//! What Driver Profiles' backend tests collect and publish: the roster Cortex's fixture
//! collects for the DSP, published as the keeper publishes it.
use crate::{
    DriverProfilesStore,
    backend::{photos, storage::bind_roster_request},
};
use dispatch_core::{db::Store, manifest::Collector};
use dispatch_cortex::associates::Capture;

/// The roster Cortex's fixture collects for the DSP.
pub(crate) fn roster(store: &Store, dsp: &str) -> Capture {
    let request = bind_roster_request(store, dsp).unwrap();
    let collected = dispatch_cortex::COLLECTOR
        .fixture("America/Chicago", &request)
        .unwrap();
    serde_json::from_value(collected.data).unwrap()
}
/// What the keeper does with a finished collection: its photos' files, then the roster.
pub(crate) fn publish(store: &Store, dsp: &str, job: &str, mut capture: Capture) {
    photos::write(&photos::dir(store, dsp).unwrap(), &mut capture.photos).unwrap();
    store.publish_roster(dsp, job, &capture).unwrap();
}
