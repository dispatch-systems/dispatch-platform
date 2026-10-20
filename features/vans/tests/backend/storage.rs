//! The fleet, against a real store in a temporary directory.
use super::*;
use crate::backend::support::fleet;
use dispatch_core::testing;

#[test]
fn a_van_the_fleet_stops_listing_keeps_its_name() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, store, dsp) = testing::bootstrapped();
    assert_eq!(
        bind_fleet_request(&store, &dsp).unwrap_err().code,
        "vans_abbreviation_required"
    );
    store
        .set_profile(&dsp, json!({"abbreviation":"FXTR"}))
        .unwrap();
    store
        .publish_fleet(&dsp, "job-1", &fleet(&store, &dsp, 1_790_830_800_000))
        .unwrap();
    let mut next = fleet(&store, &dsp, 1_790_917_200_000);
    next.vans.retain(|van| van.name.as_deref() != Some("214"));
    store.publish_fleet(&dsp, "job-2", &next).unwrap();
    let status = store.fleet_status(&dsp).unwrap();
    assert_eq!(
        (status.collected_at.as_deref(), status.listed),
        (Some("2026-10-02T05:00:00.000Z"), 5)
    );
    let db = store.vans_db(&dsp).unwrap();
    assert_eq!(
        db.query_as::<(String, Option<String>, String)>(
            "SELECT vin,name,last_listed FROM vans WHERE listed=0",
            []
        )
        .unwrap(),
        [(
            "7FCTGAAA1NN000214".to_owned(),
            Some("214".to_owned()),
            "2026-10-01T05:00:00.000Z".to_owned()
        )]
    );
}
