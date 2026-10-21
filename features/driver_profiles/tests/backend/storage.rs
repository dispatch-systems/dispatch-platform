//! The roster and its photos, against a real store in a temporary directory.
use super::*;
use crate::backend::{
    photos,
    support::{publish, roster},
};
use dispatch_core::testing;

#[test]
fn a_photo_stays_while_amazon_lists_whoever_it_shows() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert_eq!(
        bind_roster_request(&store, &dsp).unwrap_err().code,
        "driver_profiles_abbreviation_required"
    );
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    publish(&store, &dsp, "job-1", roster(&store, &dsp));
    let dir = photos::dir(&store, &dsp).unwrap();
    let files = || std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(files(), 8);
    // Every photo stands, so the next collection downloads none of them.
    assert_eq!(store.kept_photos(&dsp, "company-fixture").unwrap().len(), 8);
    // Amazon stops listing two; one more is inactive but still listed, and keeps theirs.
    let mut next = roster(&store, &dsp);
    next.associates
        .retain(|a| !["driver-7", "driver-8"].contains(&a.transporter_id.as_str()));
    next.photos.clear();
    publish(&store, &dsp, "job-2", next);
    assert_eq!(files(), 6);
    let db = store.driver_profiles_db(&dsp).unwrap();
    let count = |sql: &str| db.count(sql, []).unwrap();
    assert_eq!(
        (
            count("SELECT count(*) FROM associates WHERE listed=1 AND status='ACTIVE'"),
            count("SELECT count(*) FROM associates WHERE listed=1 AND status<>'ACTIVE'"),
            count("SELECT count(*) FROM photos")
        ),
        (5, 1, 6)
    );
    // Whoever the roster no longer lists keeps their name, for what still names them.
    assert_eq!(count("SELECT count(*) FROM associates WHERE listed=0"), 2);
}
