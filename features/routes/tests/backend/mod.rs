use super::*;
use crate::testing;
use dispatch_cortex::{
    discovery::CollectionRequest,
    routes::{MAX_ITINERARIES, fixture},
};
#[test]
fn retained_sweep_selection_rechecks_running_jobs_and_publication_activity() {
    crate::testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dispatch_core::db::private_dir(root.path()).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    config.fixture = true;
    config.development = true;
    config.environment = "preview".into();
    let db = Store::initialize(config).unwrap();
    let bootstrap = dispatch_core::server::operations::bootstrap(
        &db,
        "sweep@example.test",
        "Sweep",
        "Owner",
        "sweep-password-test",
    )
    .unwrap();
    let dsp = s(&bootstrap["dsp"], "id");
    db.set_profile(
        dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    testing::ready_connection(&db, dsp, dispatch_cortex::PROVIDER).unwrap();
    let queued = db
        .enqueue_routes(
            dsp,
            None,
            "sweep-selection",
            Some("2026-09-25"),
            Mode::Final,
            1,
        )
        .unwrap();
    let job = s(&queued[0], "id");
    let staged = db
        .stage_routes(dsp, job, fixture(&request(Mode::Final)).unwrap())
        .unwrap();
    testing::set_job_status(&db, job, "failed").unwrap();
    let mut selected = None;
    assert!(sweep_routes_step(&db, dsp, &mut selected).unwrap());
    assert_eq!(selected.as_deref(), Some(staged.publication.as_str()));
    let remaining = || {
        db.routes_db(dsp)
            .unwrap()
            .count("SELECT count(*) FROM stops", [])
            .unwrap()
    };
    let stops = remaining();
    assert!(stops > 0);
    // An intervening job transition must protect even the retained candidate.
    testing::set_job_status(&db, job, "queued").unwrap();
    assert!(!sweep_routes_step(&db, dsp, &mut selected).unwrap());
    assert_eq!(remaining(), stops);
    assert!(selected.is_none());
    testing::set_job_status(&db, job, "failed").unwrap();
    selected = Some(staged.publication.clone());
    db.routes_db(dsp)
        .unwrap()
        .exec(
            "UPDATE route_publications SET active=1 WHERE id=?",
            [&staged.publication],
        )
        .unwrap();
    assert!(!sweep_routes_step(&db, dsp, &mut selected).unwrap());
    assert_eq!(remaining(), stops);
    assert!(selected.is_none());
    db.routes_db(dsp)
        .unwrap()
        .exec(
            "UPDATE route_publications SET active=0 WHERE id=?",
            [&staged.publication],
        )
        .unwrap();
    while sweep_routes_step(&db, dsp, &mut selected).unwrap() {}
    assert!(selected.is_none());
    assert_eq!(
        db.routes_db(dsp)
            .unwrap()
            .count("SELECT count(*) FROM route_publications", [])
            .unwrap(),
        0
    );
}
fn request(mode: Mode) -> Request {
    Request {
        collection: Collection::Routes,
        mode,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Northline Logistics".into(),
        dsp_abbreviation: "NLOG".into(),
        service_area_id: None,
        provider: None,
    }
}
#[test]
fn requests_are_recognized_and_carry_a_scope_once_resolved() {
    assert!(
        Request::parse(&json!({"date":"2026-09-25"}))
            .unwrap()
            .is_none()
    );
    let value = serde_json::to_value(request(Mode::Final)).unwrap();
    assert_eq!(value["collection"], "routes");
    assert_eq!(value["mode"], "final");
    assert!(value.get("serviceAreaId").is_none());
    let parsed = Request::parse(&value).unwrap().unwrap();
    assert!(matches!(
        parsed.scope_request(),
        CollectionRequest::Discover(_)
    ));
    let mut scoped = request(Mode::Snapshot);
    scoped.service_area_id = Some("area-1".into());
    scoped.provider = Some("provider-1".into());
    assert!(matches!(
        scoped.scope_request(),
        CollectionRequest::Scoped(_)
    ));
    let mut half = request(Mode::Final);
    half.provider = Some("provider-1".into());
    assert!(half.validate().is_err());
    assert!(
        Request::parse(&json!({"collection":"routes","date":"2026-09-25","station":"tst1"}))
            .is_err()
    );
}
#[test]
fn days_follow_the_mode() {
    let today = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
    assert_eq!(default_day(Mode::Final, today), "2026-09-25");
    assert_eq!(default_day(Mode::Snapshot, today), "2026-09-26");
    assert!(day_allowed(Mode::Final, "2026-09-25", today).is_ok());
    assert!(day_allowed(Mode::Final, "2026-09-26", today).is_err());
    assert!(day_allowed(Mode::Snapshot, "2026-09-26", today).is_ok());
    assert!(day_allowed(Mode::Snapshot, "2026-09-25", today).is_err());
    assert!(day_allowed(Mode::Final, "2026-9-25", today).is_err());
}
#[test]
fn the_fixture_is_a_valid_day_whose_rows_add_up() {
    let capture = fixture(&request(Mode::Final)).unwrap();
    assert_eq!(capture.itineraries.len(), 2);
    let (stops, tasks, day) = rows(
        &capture.itineraries[1].parsed().unwrap()["itineraryDetails"],
        "driver-2",
    );
    assert_eq!((stops.len(), tasks.len()), (2, 4));
    assert_eq!((day.delivered, day.picked_up, day.not_delivered), (1, 2, 1));
    assert_eq!((day.stops_total, day.stops_completed), (2, 2));
    assert_eq!(tasks[0].tracking_id.as_deref(), Some("TBA000000000001"));
    assert_eq!(stops[0].flags, json!(["dispatchStopFlag"]));
    assert!(day.first_stop_at.unwrap() <= day.last_stop_at.unwrap());
    let inflated = gunzip(&gzip(b"{\"a\":1}").unwrap()).unwrap();
    assert_eq!(inflated, b"{\"a\":1}");
}
#[test]
fn aggregate_response_bytes_stop_at_the_daily_limit() {
    let chunk = MAX_CAPTURE_BYTES / MAX_ITINERARIES;
    let mut total = 0;
    for _ in 0..MAX_ITINERARIES {
        total = add_capture_bytes(total, chunk).unwrap();
    }
    total = add_capture_bytes(total, MAX_CAPTURE_BYTES - total).unwrap();
    assert_eq!(total, MAX_CAPTURE_BYTES);
    let error = add_capture_bytes(total, 1).unwrap_err();
    assert_eq!(error.code, "routes_source_too_large");
}
#[test]
fn stamps_take_seconds_and_milliseconds() {
    assert_eq!(stamp(&json!(1_758_800_000)), Some(1_758_800_000_000));
    assert_eq!(stamp(&json!(1_758_800_000_123i64)), Some(1_758_800_000_123));
    assert_eq!(stamp(&json!(null)), None);
    assert_eq!(stamp(&json!(-5)), None);
}
