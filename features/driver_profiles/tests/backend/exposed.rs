//! What Driver Profiles exposes, as a feature using it finds it: only while it's on for the
//! DSP.
use dispatch_core::testing;

#[test]
fn amazon_roster_is_offered_only_while_driver_profiles_is_on() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let offered = dispatch_driver_profiles_contract::for_dsp(&db, &dsp)
        .unwrap()
        .expect("Driver Profiles is on");
    // TODO(scaffold): prove each of its readers and actions against what it keeps.
    assert!(offered.ready(&db, &dsp).unwrap());
    let owner = testing::a_user(&db);
    db.set_feature(&dsp, "driver_profiles", false, &owner)
        .unwrap();
    assert!(
        dispatch_driver_profiles_contract::for_dsp(&db, &dsp)
            .unwrap()
            .is_none()
    );
}
