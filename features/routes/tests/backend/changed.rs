//! The days changed that nothing was at hand to tell of: each let go once told, but a note made
//! since, however tellings overlap.
use super::*;
use dispatch_core::testing;

#[test]
fn a_day_noted_again_between_two_overlapping_tellings_stays_to_be_told() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let lease = db.routes_db(&dsp).unwrap();
    let routes = &*lease;
    let day = "2026-11-03";
    note(routes, day).unwrap();
    // Two tellings read the day's note, and the first lets go of it once it's told.
    let (first, second) = (untold(routes).unwrap(), untold(routes).unwrap());
    told(routes, day, first[0].1).unwrap();
    // A later collection makes a newer read the day's, noting it anew; then the second telling,
    // done with the note it read, lets go of that one.
    note(routes, day).unwrap();
    told(routes, day, second[0].1).unwrap();
    let left: Vec<String> = untold(routes)
        .unwrap()
        .into_iter()
        .map(|(day, _)| day)
        .collect();
    assert_eq!(left, [day]);
}
