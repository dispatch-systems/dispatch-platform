//! The first schedule, against a real store in a temporary directory.
use super::*;
use dispatch_core::testing;

#[test]
fn a_dsp_gets_its_first_schedule_once() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    store
        .set_feature(&dsp, "vans", false, &testing::a_user(&store))
        .unwrap();
    // Without Vans, nothing.
    assert!(!first_schedule(&store, &dsp).unwrap());
    store
        .set_feature(&dsp, "vans", true, &testing::a_user(&store))
        .unwrap();
    testing::enable_connection(&store, &dsp, dispatch_cortex::PROVIDER).unwrap();
    // Nothing to collect by yet: it waits.
    assert!(!first_schedule(&store, &dsp).unwrap());
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    assert!(first_schedule(&store, &dsp).unwrap());
    let schedules = store.collection_schedules(&dsp).unwrap().schedules;
    assert_eq!(schedules.len(), 1);
    let made = &schedules[0];
    assert_eq!(
        (
            made.collection.as_str(),
            made.local_time.as_str(),
            made.enabled
        ),
        ("vans", FIRST_RUN, true)
    );
    // Removed, it never comes back.
    store
        .delete_collection_schedule(&dsp, &made.id, &json!({"revision": made.revision}))
        .unwrap();
    assert!(!first_schedule(&store, &dsp).unwrap());
    assert!(
        store
            .collection_schedules(&dsp)
            .unwrap()
            .schedules
            .is_empty()
    );
}
