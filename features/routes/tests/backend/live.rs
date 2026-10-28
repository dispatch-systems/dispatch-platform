//! Keeping a live read: today is judged as it's written, not as its reading started.
use super::*;
use crate::testing;

#[test]
fn a_read_whose_day_ended_before_it_was_written_is_refused_changing_nothing() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    // Read and shaped as yesterday's snapshot, as one is until its day ends where the DSP is.
    let yesterday = (routes_today(&db, &dsp).unwrap() - Duration::days(1)).to_string();
    let request = day_request(&db, &dsp, &yesterday, Mode::Snapshot).unwrap();
    let read = dispatch_cortex::routes::fixture(&request).unwrap();
    let mut shaped = Shaped::new(read, &request).unwrap();
    let refused = keep(&db, &dsp, &mut shaped).unwrap_err();
    assert_eq!(refused.code, "routes_day_not_available");
    let live = db.routes_live_db(&dsp).unwrap();
    for table in ["route_publications", "itineraries", "drivers"] {
        let held = live
            .count(&format!("SELECT count(*) FROM {table}"), [])
            .unwrap();
        assert_eq!(held, 0, "{table}");
    }
}
