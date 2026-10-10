//! Daily Performance's collection queue: its default refresh and its backfills.
use dispatch_core::{db::Store, testing};
use dispatch_cortex::{self as cortex, daily_performance};
use dispatch_daily_performance::DailyPerformanceStore;
use serde_json::json;
fn ready() -> (tempfile::TempDir, Store, String) {
    dispatch_backend::install();
    let (root, store, dsp) = testing::bootstrapped();
    testing::set_dsp(&store, &dsp, "Fixture Delivery", "America/Los_Angeles").unwrap();
    store
        .set_profile(
            &dsp,
            json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
        )
        .unwrap();
    testing::ready_connection(&store, &dsp, cortex::PROVIDER).unwrap();
    store.enable_all_features(&dsp).unwrap();
    (root, store, dsp)
}
#[test]
fn the_default_daily_refresh_can_queue_all_seven_dates() {
    let (_root, store, dsp) = ready();
    let requests: Vec<_> = store
        .daily_performance_jobs(&dsp)
        .unwrap()
        .into_iter()
        .map(|(key, request)| (key, cortex::PROVIDER, request))
        .collect();
    assert_eq!(requests.len(), 7);
    assert_eq!(store.enqueue_batch(&dsp, None, &requests).unwrap().len(), 7);
}
#[test]
fn daily_backfills_admit_31_dates_atomically_with_idempotency_and_existing_bounds() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(
            &dsp,
            Some(&testing::platform_owner(&store)),
            "month",
            "2026-09-01",
            "2026-10-01",
        )
        .unwrap();
    assert_eq!(queued.as_array().unwrap().len(), 31);
    assert_eq!(
        store
            .enqueue_daily_performance(
                &dsp,
                Some(&testing::platform_owner(&store)),
                "month",
                "2026-09-01",
                "2026-10-01"
            )
            .unwrap(),
        queued
    );
    assert_eq!(
        store
            .enqueue_daily_performance(&dsp, None, "overflow", "2026-10-02", "2026-10-02")
            .unwrap_err()
            .code,
        "queue_full"
    );
    assert_eq!(
        store
            .recent_jobs_of(&dsp, daily_performance::JOB_KIND)
            .unwrap()
            .len(),
        31
    );
    assert_eq!(
        store
            .enqueue_daily_performance(&dsp, None, "too-long", "2026-09-01", "2026-10-02")
            .unwrap_err()
            .code,
        "invalid_date_range"
    );
}
