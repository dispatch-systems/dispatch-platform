//! Daily isolation, publication safety and scheduling.
use dispatch_core::{
    db::{Store, s},
    testing,
};
use dispatch_cortex::{
    self as cortex,
    daily_performance::{self, Request},
    discovery::CollectionRequest,
};
use dispatch_daily_performance::{DailyPerformancePolicy, DailyPerformanceStore};
use serde_json::{Value, json};
fn ready() -> (tempfile::TempDir, Store, String) {
    testing::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_daily_performance::FEATURE],
    );
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
fn publish(store: &Store, dsp: &str, date: &str, key: &str) -> daily_performance::Capture {
    let queued = store
        .enqueue_daily_performance(dsp, None, key, date, date)
        .unwrap();
    let job = s(&queued[0], "id");
    let request: Value =
        serde_json::from_str(&store.job_row(job, Some(dsp)).unwrap().request).unwrap();
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let request: Request =
        serde_json::from_value(keeper.bind(store, dsp, &request).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!("discover")
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let capture = daily_performance::fixture(&request).unwrap();
    store
        .publish_daily_performance(dsp, job, &capture, &scope)
        .unwrap();
    capture
}
#[test]
fn unknown_stored_dataset_returns_an_error_without_panicking() {
    let (_root, store, dsp) = ready();
    publish(&store, &dsp, "2026-10-03", "unknown-dataset");
    store
        .daily_performance_db(&dsp)
        .unwrap()
        .exec(
            "UPDATE daily_datasets SET dataset='retired_dataset' WHERE dataset='driver_quality'",
            [],
        )
        .unwrap();
    let error = store.daily_performance_days(&dsp).unwrap_err();
    assert_eq!(error.code, "daily_performance_unknown_stored_dataset");
    assert_eq!(error.status, 503);
}
#[test]
fn daily_publications_supersede_without_losing_history_and_empty_datasets_are_unconfirmed() {
    let (_root, store, dsp) = ready();
    let first = publish(&store, &dsp, "2026-10-03", "first");
    let job = store
        .enqueue_daily_performance(&dsp, None, "first", "2026-10-03", "2026-10-03")
        .unwrap();
    let request = Request {
        collection: daily_performance::Collection::DailyPerformance,
        date: "2026-10-03".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Delivery".into(),
        dsp_abbreviation: "FXTR".into(),
    };
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!()
    };
    store
        .publish_daily_performance(
            &dsp,
            s(&job[0], "id"),
            &first,
            &discovery.scope("area-fixture", "company-fixture").unwrap(),
        )
        .unwrap();
    assert_eq!(
        store
            .daily_performance_db(&dsp)
            .unwrap()
            .count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        1
    );
    publish(&store, &dsp, "2026-10-03", "again");
    let data = store.daily_performance_db(&dsp).unwrap();
    assert_eq!(
        data.count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        2
    );
    assert_eq!(
        data.count("SELECT count(*) FROM daily_publications WHERE active=1", [])
            .unwrap(),
        1
    );
    assert_eq!(
        data.count("SELECT count(*) FROM daily_rows", []).unwrap(),
        2 * first.row_count() as i64
    );
    let days = store.daily_performance_days(&dsp).unwrap();
    assert_eq!(days.days.len(), 1);
    let contacts = days.days[0]
        .datasets
        .iter()
        .find(|dataset| dataset.dataset == "driver_contacts")
        .unwrap();
    assert_eq!(
        (contacts.rows, contacts.coverage.as_str()),
        (0, "unconfirmed")
    );
}
#[test]
fn cross_date_and_cross_company_rows_are_rejected_before_any_publication_changes() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(&dsp, None, "scope", "2026-10-03", "2026-10-03")
        .unwrap();
    let job = s(&queued[0], "id");
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let payload: Value =
        serde_json::from_str(&store.job_row(job, Some(&dsp)).unwrap().request).unwrap();
    let request: Request =
        serde_json::from_value(keeper.bind(&store, &dsp, &payload).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!()
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let capture = daily_performance::fixture(&request).unwrap();
    for (field, value) in [
        ("data_date", "2026-10-02"),
        ("company_id", "foreign"),
        ("station_code", "OTHER"),
    ] {
        let mut wrong = capture.clone();
        wrong.datasets[0].rows[0][field] = json!(value);
        assert!(
            store
                .publish_daily_performance(&dsp, job, &wrong, &scope)
                .is_err()
        );
    }
    assert_eq!(
        store
            .daily_performance_db(&dsp)
            .unwrap()
            .count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        0
    );
}
#[test]
fn schedules_refresh_configured_dates_and_manual_backfills_are_bounded_and_idempotent() {
    let (_root, store, dsp) = ready();
    store
        .set_daily_performance_policy(
            &dsp,
            &DailyPerformancePolicy {
                lookback_days: 3,
                refresh_hours: 20,
            },
        )
        .unwrap();
    let jobs = store.daily_performance_jobs(&dsp).unwrap();
    assert_eq!(jobs.len(), 3);
    let date = s(&jobs[0].1, "date").to_owned();
    publish(&store, &dsp, &date, "recent");
    assert_eq!(store.daily_performance_jobs(&dsp).unwrap().len(), 2);
    store
        .daily_performance_db(&dsp)
        .unwrap()
        .exec(
            "UPDATE daily_publications SET collected_at='2020-01-01'",
            [],
        )
        .unwrap();
    assert_eq!(store.daily_performance_jobs(&dsp).unwrap().len(), 3);
    let first = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 3);
    let again = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first, again);
    assert!(
        store
            .enqueue_daily_performance(&dsp, None, "huge", "2026-01-01", "2026-10-03")
            .is_err()
    );
    assert!(
        store
            .set_daily_performance_policy(
                &dsp,
                &DailyPerformancePolicy {
                    lookback_days: 0,
                    refresh_hours: 20
                }
            )
            .is_err()
    );
}
