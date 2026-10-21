//! The fleet, against a real store in a temporary directory.
use super::*;
use crate::backend::support::fleet;
use dispatch_core::testing;

#[test]
fn the_fleet_is_collected_by_the_dsps_abbreviation_and_counts_what_it_lists() {
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
    next.vans
        .retain(|van| van.name.as_deref() != Some("EDV 214"));
    store.publish_fleet(&dsp, "job-2", &next).unwrap();
    // The newest collection, and the vans it lists, those the DSP runs apart from the rest.
    let db = store.vans_db(&dsp).unwrap();
    let count = |sql: &str| db.count(sql, []).unwrap();
    let newest: Vec<(String,)> = db
        .query_as(
            "SELECT collected_at FROM fleet_runs ORDER BY collected_at DESC LIMIT 1",
            [],
        )
        .unwrap();
    assert_eq!(
        (
            newest[0].0.as_str(),
            count("SELECT count(*) FROM vans WHERE listed=1 AND status<>'INACTIVE'"),
            count("SELECT count(*) FROM vans WHERE listed=1 AND status='INACTIVE'")
        ),
        ("2026-10-02T05:00:00.000Z", 4, 1)
    );
}
