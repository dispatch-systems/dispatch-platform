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
#[test]
fn a_save_still_staging_is_kept_from_the_sweep() {
    testing::install(&[&dispatch_cortex::COLLECTOR], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let mut lists = fixture(&request(Mode::Final)).unwrap();
    let itineraries = std::mem::take(&mut lists.itineraries).len();
    // A save's publication is no job's: only its place among the saves staging keeps it.
    let saving = saving::Saving::start(&dsp, "save-1").unwrap();
    db.stage_routes_start(&dsp, "save-1", &lists, itineraries)
        .unwrap();
    assert_eq!(
        saving::Saving::start(&dsp, "save-1").unwrap_err().code,
        "routes_save_running"
    );
    assert!(!sweep_routes_step(&db, &dsp, &mut None).unwrap());
    drop(saving);
    while sweep_routes_step(&db, &dsp, &mut None).unwrap() {}
    let publications = db
        .routes_db(&dsp)
        .unwrap()
        .count("SELECT count(*) FROM route_publications", [])
        .unwrap();
    assert_eq!(publications, 0);
}
/// Queues `day`, stages the fixture's day for that job and publishes it, as the executor does.
pub(super) fn publish(db: &Store, dsp: &str, day: &str, mode: Mode) {
    let jobs = db
        .enqueue_routes(dsp, None, day, Some(day), mode, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    let request = serde_json::from_str(&db.job_row(&job, Some(dsp)).unwrap().request).unwrap();
    let capture = fixture(&Request::parse(&request).unwrap().unwrap()).unwrap();
    let staged = db.stage_routes(dsp, &job, capture).unwrap();
    db.publish_routes(dsp, &job, &staged).unwrap();
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
    let shaped = Shaper::new(&capture)
        .shape(&capture.itineraries[1])
        .unwrap();
    let (stops, tasks, day) = (&shaped.stops, &shaped.tasks, &shaped.day);
    assert_eq!((stops.len(), tasks.len()), (2, 4));
    assert_eq!((day.delivered, day.picked_up, day.not_delivered), (1, 2, 1));
    assert_eq!((day.stops_total, day.stops_completed), (2, 2));
    assert_eq!(tasks[0].tracking_id.as_deref(), Some("TBA000000000001"));
    assert_eq!(stops[0].flags, json!(["dispatchStopFlag"]));
    assert!(day.first_stop_at.unwrap() <= day.last_stop_at.unwrap());
    let inflated = gunzip(&gzip(b"{\"a\":1}").unwrap()).unwrap();
    assert_eq!(inflated, b"{\"a\":1}");
}
/// A stop as a JSON tree holds it, read as shaping read every itinerary before `wire`: what
/// the typed reading answers as.
impl StopFields for Value {
    type Task = Value;
    fn field(&self, key: &str) -> &Value {
        &self[key]
    }
    fn tasks(&self) -> impl Iterator<Item = &Value> {
        self["tasks"].as_array().into_iter().flatten()
    }
    fn flags(&self) -> Value {
        let mut names: Vec<&str> = self
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, v)| (k.ends_with("Flag") || k.starts_with("is")) && **v == json!(true))
            .map(|(k, _)| k.as_str())
            .collect();
        names.sort_unstable();
        json!(names)
    }
}
/// A task as a JSON tree holds it.
impl TaskFields for Value {
    fn field(&self, key: &str) -> &Value {
        &self[key]
    }
    fn inner(&self, object: &str, key: &str) -> &Value {
        &self[object][key]
    }
    fn events(&self) -> Value {
        Value::Array(
            self["recentTaskEvents"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| json!([e["executionTime"], e["taskState"], e["taskStateContext"]]))
                .collect(),
        )
    }
}
#[test]
fn an_itinerary_read_straight_from_its_response_shapes_as_from_its_tree() {
    let capture = fixture(&request(Mode::Snapshot)).unwrap();
    let shaper = Shaper::new(&capture);
    // Fields missing, repeated, of another kind than expected, or not objects at all.
    let odd = json!({"itineraryDetails":{"itineraryId":"it-9","transporterId":"driver-1","routeCode":"CX9",
        "stopProgress":{"completed":"1"},"plannedBreaks":[{"breakId":"b1","plannedStart":1758800000}],
        "stops":[null, 5, {"stopId":"s1","sequenceNumber":"3","dispatchStopFlag":true,"isLocker":true,
            "isOpen":"yes","aFlag":true,"aFlag":false,"bFlag":"true","routeCode":["CX9"],
            "tasks":[{"taskId":"t1","transporterId":"driver-1","domainMap":"none","weight":"1.5","length":2,
                "taskType":"DROP_OFF","taskState":"DELIVERED","actualExecutionTime":1758800000.5,
                "recentTaskEvents":[{"executionTime":{"at":1},"taskState":"X"},7]},
                {"taskId":"t1","taskType":"PICK_UP"},{"taskId":["t5"]},{"taskId":"t2","transporterId":"driver-2"},
                {"taskId":"t3","taskId":"t4","executionGeocode":{"latitude":"47.1","longitude":[1]},
                "highValueTaskFlag":true,"timeWindowed":"true","recentTaskEvents":{"at":1}}]},
            {"sequenceNumber":4,"tasks":"none"}],
        "inactiveTasks":[{"taskId":"t9"},{"taskId":"t9","taskState":"X"},"t8",{"taskId":"t4"}]},
        "addresses":[{"addressId":"a1","geocode":{"latitude":1}}],"transporters":"none"});
    let odd = ItineraryCapture {
        id: "it-9".into(),
        transporter_id: "driver-1".into(),
        detail: odd.to_string(),
    };
    for captured in capture.itineraries.iter().chain([&odd]) {
        let typed = shaper.shape(captured).unwrap();
        let tree: Value = serde_json::from_str(&captured.detail).unwrap();
        let detail = &tree["itineraryDetails"];
        let (stops, tasks, day) = stop_rows(
            detail["stops"].as_array().into_iter().flatten(),
            &detail["stopProgress"],
            &captured.transporter_id,
        );
        let read = Read {
            detail,
            transporters: &tree["transporters"],
            addresses: &tree["addresses"],
            stops,
            tasks,
            day,
            inactive: inactive_rows(detail["inactiveTasks"].as_array().into_iter().flatten()),
        };
        let from_tree = shaper
            .shape_read(
                &captured.id,
                &captured.transporter_id,
                read,
                captured.detail.len(),
                typed.raw.clone(),
            )
            .unwrap();
        assert!(!from_tree.tasks.is_empty() && !from_tree.stops.is_empty());
        assert_eq!(
            serde_json::to_string(&typed).unwrap(),
            serde_json::to_string(&from_tree).unwrap()
        );
    }
    // A detail that isn't an object is no itinerary.
    let refused = shaper.shape(&ItineraryCapture {
        detail: json!({"itineraryDetails":[]}).to_string(),
        ..odd
    });
    assert_eq!(refused.unwrap_err().code, "routes_capture_invalid");
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
