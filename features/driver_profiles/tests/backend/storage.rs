//! The roster and its photos, against a real store in a temporary directory.
use super::*;
use crate::backend::photos;
use dispatch_core::{manifest::Collector, testing};

/// The roster Cortex's fixture collects for the DSP.
fn roster(store: &Store, dsp: &str) -> Capture {
    let request = bind_roster_request(store, dsp).unwrap();
    let collected = dispatch_cortex::COLLECTOR
        .fixture("America/Chicago", &request)
        .unwrap();
    serde_json::from_value(collected.data).unwrap()
}
/// What the keeper does with a finished collection: its photos' files, then the roster.
fn publish(store: &Store, dsp: &str, job: &str, mut capture: Capture) {
    photos::write(&photos::dir(store, dsp).unwrap(), &mut capture.photos).unwrap();
    store.publish_roster(dsp, job, &capture).unwrap();
}

#[test]
fn a_photo_stays_while_amazon_lists_whoever_it_shows() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert_eq!(
        bind_roster_request(&store, &dsp).unwrap_err().code,
        "driver_profiles_abbreviation_required"
    );
    store.set_profile(&dsp, json!({"abbreviation":"FXTR"})).unwrap();
    publish(&store, &dsp, "job-1", roster(&store, &dsp));
    let dir = photos::dir(&store, &dsp).unwrap();
    let files = || std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(files(), 8);
    // Every photo stands, so the next collection downloads none of them.
    assert_eq!(
        store.kept_photos(&dsp, "company-fixture").unwrap().len(),
        8
    );
    // Amazon stops listing two; one more is inactive but still listed, and keeps theirs.
    let mut next = roster(&store, &dsp);
    next.associates
        .retain(|a| !["driver-7", "driver-8"].contains(&a.transporter_id.as_str()));
    next.photos.clear();
    publish(&store, &dsp, "job-2", next);
    assert_eq!(files(), 6);
    let status = store.roster_status(&dsp).unwrap();
    assert_eq!((status.active, status.inactive, status.photos), (5, 1, 6));
    // Whoever the roster no longer lists keeps their name, for what still names them.
    let db = store.driver_profiles_db(&dsp).unwrap();
    assert_eq!(
        db.count("SELECT count(*) FROM associates WHERE listed=0", [])
            .unwrap(),
        2
    );
}
