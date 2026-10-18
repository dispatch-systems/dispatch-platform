//! What Timecard exposes, as a feature using it finds it: only while it's on for the
//! DSP.
use dispatch_core::testing;

#[test]
fn timecards_is_offered_only_while_timecard_is_on() {
    testing::install(
        &[&dispatch_cortex::COLLECTOR, &dispatch_paycom::COLLECTOR],
        &[&crate::FEATURE],
    );
    let (_root, db, dsp) = testing::bootstrapped();
    let offered = dispatch_timecard_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Timecard is on");
    // TODO(scaffold): prove each of its readers and actions against what it keeps.
    assert!(offered.ready(&db, &dsp).unwrap());
    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "timecard", false, &owner).unwrap();
    assert!(
        dispatch_timecard_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}
