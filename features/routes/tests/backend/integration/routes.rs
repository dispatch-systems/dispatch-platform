//! Routes storage: publication, supersession, the days a schedule queues and what the
//! views read.
use dispatch_backend::routedata::{self, RoutesStore};
use dispatch_core::db::{Store, s};
use dispatch_core::testing as common;
use dispatch_cortex::{
    self as cortex,
    routes::{self, Capture, Mode, Request},
};
use serde_json::{Value, json};

/// Routes, and the Cortex collector whose routes it keeps.
fn install() {
    common::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_backend::feature_manifests::routes::FEATURE],
    );
}

fn request(day: &str, mode: Mode) -> Request {
    Request::parse(
        &json!({"collection":"routes","mode":mode.as_str(),"date":day,"station":"TST1",
        "timezone":"UTC","dspName":"Test Owner","dspAbbreviation":"NLOG"}),
    )
    .unwrap()
    .unwrap()
}
/// A DSP with a station and an enabled Cortex connection.
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &id, cortex::PROVIDER).unwrap();
    (root, db, id)
}
/// Queues `day`, stages `capture` for that job and publishes it, as the executor does.
fn publish(db: &Store, id: &str, key: &str, day: &str, capture: &Capture) -> String {
    let jobs = db
        .enqueue_routes(id, None, key, Some(day), capture.mode, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    let staged = db.stage_routes(id, &job, capture.clone()).unwrap();
    db.publish_routes(id, &job, &staged).unwrap();
    // The executor publishes in the same transaction that finishes the job.
    common::set_job_status(db, &job, "succeeded").unwrap();
    job
}
/// Runs the sweep until nothing is left for it.
fn sweep(db: &Store, id: &str) -> usize {
    let mut steps = 0;
    while db.sweep_routes(id).unwrap() {
        steps += 1;
    }
    steps
}
fn count(db: &Store, id: &str, sql: &str) -> i64 {
    db.routes_db(id).unwrap().count(sql, []).unwrap()
}

#[test]
fn a_day_is_published_into_normalized_rows_with_its_raw_responses() {
    install();
    let (_root, db, id) = ready();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let job = publish(&db, &id, "first", "2026-09-25", &capture);
    let queued = db.job_row(&job, Some(&id)).unwrap();
    assert_eq!(queued.kind.as_str(), "cortex.routes.collect");
    let storage = db.routes_db(&id).unwrap();
    let publication = storage
        .one("SELECT * FROM route_publications WHERE job_id=?", [&job])
        .unwrap()
        .unwrap();
    assert_eq!(publication["active"], 1);
    assert_eq!(publication["day"], "2026-09-25");
    assert_eq!(publication["mode"], "final");
    assert_eq!(publication["route_count"], 2);
    assert_eq!(publication["itinerary_count"], 2);
    assert_eq!(publication["stop_count"], 4);
    assert_eq!(publication["task_count"], 8);
    let pid = s(&publication, "id");
    // Tasks carry the tracking id, the driver and the outcome.
    let tasks = storage
        .all(
            "SELECT tracking_id,task_type,task_state,transporter_id,latitude FROM tasks \
             WHERE publication_id=? AND itinerary_id='itinerary-2' AND active=1 ORDER BY task_id",
            [pid],
        )
        .unwrap();
    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks[0]["tracking_id"], "TBA000000000001");
    assert_eq!(tasks[0]["transporter_id"], "driver-2");
    assert_eq!(tasks[3]["task_state"], "UNDELIVERABLE");
    assert_eq!(tasks[3]["latitude"], 34.01);
    // A task's recent events are kept as [time, state, context] triples.
    let events: Value = serde_json::from_str(s(
        &storage
            .one(
                "SELECT events FROM tasks WHERE publication_id=? AND task_id='task-13'",
                [pid],
            )
            .unwrap()
            .unwrap(),
        "events",
    ))
    .unwrap();
    assert_eq!(events.as_array().map(Vec::len), Some(1));
    assert!(events[0][0].as_i64().unwrap() > 0);
    assert_eq!(events[0][1], "DELIVERED");
    assert_eq!(events[0][2], "NONE");
    // The day summary adds the tasks up.
    let day = storage
        .one(
            "SELECT delivered,picked_up,not_delivered,stops_total,stops_completed,departed_at FROM driver_days \
             WHERE publication_id=? AND transporter_id='driver-2'",
            [pid],
        )
        .unwrap()
        .unwrap();
    assert_eq!(day["delivered"], 1);
    assert_eq!(day["picked_up"], 2);
    assert_eq!(day["not_delivered"], 1);
    assert_eq!(day["stops_completed"], 2);
    assert!(day["departed_at"].as_i64().unwrap() > 0);
    // Addresses and drivers are shared across days; the raw bodies inflate to what was sent.
    assert_eq!(count(&db, &id, "SELECT count(*) FROM addresses"), 2);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM drivers"), 2);
    let address = storage
        .one(
            "SELECT address1,city,postal_code,latitude FROM addresses WHERE address_id='address-1'",
            [],
        )
        .unwrap()
        .unwrap();
    assert_eq!(address["address1"], "100 Example St");
    assert_eq!(address["postal_code"], "90000");
    // Amazon sends customer names and phones empty; no column holds them.
    let columns: Vec<String> = storage
        .all("PRAGMA table_info(addresses)", [])
        .unwrap()
        .iter()
        .map(|c| s(c, "name").to_owned())
        .collect();
    assert!(!columns.iter().any(|c| c.starts_with("customer")));
    let raw = storage
        .all(
            "SELECT name,raw_bytes FROM route_raw WHERE publication_id=? ORDER BY name",
            [pid],
        )
        .unwrap();
    assert_eq!(
        raw.iter()
            .map(|r| s(r, "name").to_owned())
            .collect::<Vec<_>>(),
        [
            "itinerary:itinerary-1",
            "itinerary:itinerary-2",
            "route_summaries",
            "summaries"
        ]
    );
    let body: Vec<u8> = storage
        .0
        .query_row(
            "SELECT body FROM route_raw WHERE publication_id=? AND name='summaries'",
            [pid],
            |row| row.get(0),
        )
        .unwrap();
    let inflated: Value = serde_json::from_slice(&routedata::gunzip(&body).unwrap()).unwrap();
    assert_eq!(inflated, capture.summaries);
    // The views read the publication and its drivers.
    let days = db.route_days(&id).unwrap();
    assert_eq!(days.station, "TST1");
    assert_eq!(days.days.len(), 1);
    assert_eq!(days.days[0].task_count, 8);
    let view = db.route_day(&id, "2026-09-25").unwrap().unwrap();
    assert_eq!(view.itineraries.len(), 2);
    assert_eq!(view.itineraries[0].driver_name, "Fixture Driver");
    assert_eq!(view.itineraries[0].delivered, 2);
    assert_eq!(view.itineraries[1].not_delivered, 1);
    assert!(db.route_day(&id, "2026-09-24").unwrap().is_none());
    // Breaks, unknown stops and removed tasks have rows of their own; the views join them.
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM breaks WHERE planned=0"),
        2
    );
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM breaks WHERE planned=1"),
        2
    );
    assert_eq!(count(&db, &id, "SELECT count(*) FROM unknown_stops"), 2);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM tasks WHERE active=0"),
        2
    );
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM deliveries WHERE task_state='DELIVERED' AND address1 IS NOT NULL"
        ),
        3
    );
    // One stop per driver had deliveries; the first driver's had two.
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM driver_stops WHERE delivered>0"
        ),
        2
    );
    assert_eq!(
        count(&db, &id, "SELECT sum(delivered) FROM driver_stops"),
        3
    );
    let detail = db
        .route_itinerary(&id, "2026-09-25", "itinerary-2")
        .unwrap()
        .unwrap();
    assert_eq!(detail.stops.len(), 2);
    assert_eq!(detail.stops[1].tasks.len(), 2);
    assert_eq!(
        detail.stops[1].address.as_ref().unwrap().city.as_deref(),
        Some("Fixture")
    );
    assert_eq!(detail.breaks.len(), 2);
    assert_eq!(detail.unknown_stops.len(), 1);
    assert_eq!(detail.inactive_tasks.len(), 1);
    assert!(!detail.inactive_tasks[0].active);
    let package = db.route_package(&id, "TBA000000000002").unwrap();
    assert_eq!(package.events.len(), 4);
    assert!(
        package
            .events
            .iter()
            .any(|e| e.task.task_state.as_deref() == Some("UNDELIVERABLE"))
    );
    assert!(
        db.route_itinerary(&id, "2026-09-25", "nope")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_reprocess_rebuilds_every_row_from_the_stored_responses() {
    install();
    let (_root, db, id) = ready();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    publish(&db, &id, "first", "2026-09-25", &capture);
    let before = db.route_days(&id).unwrap().days.remove(0);
    let storage = db.routes_db(&id).unwrap();
    for table in ["tasks", "stops", "breaks", "unknown_stops", "driver_days"] {
        storage.exec(&format!("DELETE FROM {table}"), []).unwrap();
    }
    storage
        .exec(
            "UPDATE route_publications SET stop_count=0,task_count=0",
            [],
        )
        .unwrap();
    let result = db.reprocess_routes(&id, None).unwrap();
    assert_eq!(result.publications.len(), 1);
    assert_eq!(result.publications[0].stop_count, before.stop_count);
    assert_eq!(result.publications[0].task_count, before.task_count);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM tasks WHERE active=1"),
        8
    );
    assert_eq!(count(&db, &id, "SELECT count(*) FROM breaks"), 4);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM unknown_stops"), 2);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM driver_days"), 2);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM route_raw"), 4);
    assert!(
        storage
            .all("PRAGMA foreign_key_check", [])
            .unwrap()
            .is_empty()
    );
    // A day without a publication rebuilds nothing.
    assert!(
        db.reprocess_routes(&id, Some("2026-09-24"))
            .unwrap()
            .publications
            .is_empty()
    );
    assert!(db.reprocess_routes(&id, Some("bad")).is_err());
}

#[test]
fn mismatched_incomplete_and_oversized_captures_fail_before_changing_publications() {
    install();
    let (_root, db, id) = ready();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    // An itinerary that is not the one listed is refused before anything is stored.
    let mut swapped = capture.clone();
    swapped.itineraries.swap(0, 1);
    let jobs = db
        .enqueue_routes(&id, None, "swapped", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let error = db
        .stage_routes(&id, s(&jobs[0], "id"), swapped)
        .unwrap_err();
    assert_eq!(error.code, "routes_scope_mismatch");
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        0
    );
    // A staged day missing an itinerary is never made active.
    let jobs = db
        .enqueue_routes(&id, None, "partial", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    let staged = db.stage_routes(&id, job, capture.clone()).unwrap();
    db.routes_db(&id)
        .unwrap()
        .exec(
            "DELETE FROM itineraries WHERE itinerary_id='itinerary-2'",
            [],
        )
        .unwrap();
    let error = db.publish_routes(&id, job, &staged).unwrap_err();
    assert_eq!(error.code, "routes_capture_invalid");
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM route_publications WHERE active=1"
        ),
        0
    );
    assert!(db.route_days(&id).unwrap().days.is_empty());

    publish(&db, &id, "ordinary", "2026-09-25", &capture);
    let before = count(&db, &id, "SELECT count(*) FROM tasks");
    db.routes_db(&id)
        .unwrap()
        .exec(
            "UPDATE route_raw SET raw_bytes=? WHERE name LIKE 'itinerary:%'",
            [routes::MAX_CAPTURE_BYTES as i64],
        )
        .unwrap();
    let error = db.reprocess_routes(&id, None).unwrap_err();
    assert_eq!(error.code, "routes_source_too_large");
    assert_eq!(count(&db, &id, "SELECT count(*) FROM tasks"), before);
}

#[test]
fn a_package_moved_between_drivers_keeps_a_row_under_each() {
    install();
    let (_root, db, id) = ready();
    let mut capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    // The second driver delivered task-13; the first driver's itinerary lists it as
    // removed from their route, as Amazon lists a rescued package.
    let second: Value = serde_json::from_str(&capture.itineraries[1].detail).unwrap();
    let rescued = second["itineraryDetails"]["stops"][1]["tasks"][0].clone();
    assert_eq!(rescued["taskId"], "task-13");
    let mut first: Value = serde_json::from_str(&capture.itineraries[0].detail).unwrap();
    first["itineraryDetails"]["inactiveTasks"]
        .as_array_mut()
        .unwrap()
        .push(rescued);
    capture.itineraries[0].detail = first.to_string();
    publish(&db, &id, "rescue", "2026-09-25", &capture);
    let rows = db
        .routes_db(&id)
        .unwrap()
        .all(
            "SELECT itinerary_id,transporter_id,active,task_state FROM tasks WHERE task_id='task-13' \
             ORDER BY itinerary_id",
            [],
        )
        .unwrap();
    assert_eq!(
        rows,
        vec![
            json!({"itinerary_id":"itinerary-1","transporter_id":"driver-1","active":0,"task_state":"DELIVERED"}),
            json!({"itinerary_id":"itinerary-2","transporter_id":"driver-2","active":1,"task_state":"DELIVERED"}),
        ]
    );
    // The delivery counts where it happened, and its tasks agree with the day summary.
    let view = db.route_day(&id, "2026-09-25").unwrap().unwrap();
    let delivered: i64 = view.itineraries.iter().map(|i| i.delivered).sum();
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM deliveries WHERE active=1 AND task_type='DROP_OFF' AND task_state='DELIVERED'"
        ),
        delivered
    );
    let package = db.route_package(&id, "TBA000000000001").unwrap();
    assert!(
        package
            .events
            .iter()
            .any(|e| e.itinerary_id == "itinerary-2" && e.task.active)
    );
}

#[test]
fn a_recollected_day_replaces_its_previous_publication_and_keeps_shared_rows() {
    install();
    let (_root, db, id) = ready();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let first = publish(&db, &id, "first", "2026-09-25", &capture);
    let mut again = capture.clone();
    again.itineraries.truncate(1);
    again.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    again.started_at += 1000;
    again.finished_at += 2000;
    let second = publish(&db, &id, "second", "2026-09-25", &again);
    assert_ne!(first, second);
    let storage = db.routes_db(&id).unwrap();
    // Readers see only the new day at once; the sweep deletes the old one in steps.
    let days = db.route_days(&id).unwrap();
    assert_eq!(days.days.len(), 1);
    assert_eq!(days.days[0].itinerary_count, 1);
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM route_publications WHERE active=0"
        ),
        1
    );
    assert!(sweep(&db, &id) > 1);
    let publications = storage
        .all(
            "SELECT job_id,active,itinerary_count FROM route_publications",
            [],
        )
        .unwrap();
    assert_eq!(
        publications,
        vec![json!({"job_id":second,"active":1,"itinerary_count":1})]
    );
    // Rows under the replaced publication went with it; shared rows stayed.
    assert_eq!(count(&db, &id, "SELECT count(*) FROM itineraries"), 1);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM tasks WHERE active=1"),
        4
    );
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM tasks WHERE active=0"),
        1
    );
    assert_eq!(count(&db, &id, "SELECT count(*) FROM driver_days"), 1);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM route_raw"), 3);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM addresses"), 2);
    assert_eq!(count(&db, &id, "SELECT count(*) FROM drivers"), 2);
    assert!(
        storage
            .all("PRAGMA foreign_key_check", [])
            .unwrap()
            .is_empty()
    );
    // A capture for another day or station is refused.
    let other = routes::fixture(&request("2026-09-24", Mode::Final)).unwrap();
    let jobs = db
        .enqueue_routes(&id, None, "third", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    assert!(db.stage_routes(&id, s(&jobs[0], "id"), other).is_err());
}

#[test]
fn a_schedule_queues_recent_days_without_a_final_publication() {
    install();
    let (_root, db, id) = ready();
    // The bootstrapped DSP keeps UTC, so its days are UTC days.
    let today = chrono::Utc::now().date_naive();
    let day = |back: i64| (today - chrono::Duration::days(back)).to_string();
    let jobs = db.routes_jobs(&id).unwrap();
    // Yesterday first, then the days before, at most four per run.
    assert_eq!(jobs.len(), routedata::MAX_JOBS_PER_RUN);
    assert_eq!(jobs[0].0, format!("routes:{}", day(1)));
    assert_eq!(jobs[0].1["date"], day(1));
    assert_eq!(jobs[0].1["mode"], "final");
    assert_eq!(jobs[0].1["station"], "TST1");
    assert_eq!(jobs[3].0, format!("routes:{}", day(4)));
    // A published day is not asked for again.
    let capture = routes::fixture(&request(&day(1), Mode::Final)).unwrap();
    publish(&db, &id, "yesterday", &day(1), &capture);
    let jobs = db.routes_jobs(&id).unwrap();
    assert_eq!(jobs[0].0, format!("routes:{}", day(2)));
    assert!(
        jobs.iter()
            .all(|(key, _)| key != &format!("routes:{}", day(1)))
    );
    // A snapshot of today is a different reading and leaves the schedule's view alone.
    let snapshot = routes::fixture(&request(&day(0), Mode::Snapshot)).unwrap();
    publish(&db, &id, "today", &day(0), &snapshot);
    assert_eq!(db.route_days(&id).unwrap().days.len(), 2);
    assert_eq!(
        db.routes_jobs(&id).unwrap()[0].0,
        format!("routes:{}", day(2))
    );
}

#[test]
fn collection_requests_need_a_station_and_an_allowed_day() {
    install();
    let (_root, db, id) = common::bootstrapped();
    common::ready_connection(&db, &id, cortex::PROVIDER).unwrap();
    let refused = db.enqueue_routes(&id, None, "k", None, Mode::Final, 1);
    assert_eq!(refused.unwrap_err().code, "routes_station_required");
    assert_eq!(
        db.routes_schedule_ready(&id).unwrap_err().code,
        "routes_station_required"
    );
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    assert!(db.routes_schedule_ready(&id).is_ok());
    // Today cannot be read as final, and a snapshot reads today only, one day at a time.
    let today = chrono::Utc::now().date_naive().to_string();
    assert_eq!(
        db.enqueue_routes(&id, None, "k", Some(&today), Mode::Final, 1)
            .unwrap_err()
            .code,
        "routes_day_not_available"
    );
    assert_eq!(
        db.enqueue_routes(&id, None, "k", Some("2026-01-01"), Mode::Snapshot, 1)
            .unwrap_err()
            .code,
        "routes_day_not_available"
    );
    assert_eq!(
        db.enqueue_routes(&id, None, "k", None, Mode::Snapshot, 2)
            .unwrap_err()
            .code,
        "invalid_input"
    );
    // Several days queue one job each under the key, newest first.
    let jobs = db
        .enqueue_routes(&id, None, "range", Some("2026-01-05"), Mode::Final, 3)
        .unwrap();
    assert_eq!(jobs.as_array().unwrap().len(), 3);
}

#[test]
fn the_sweep_keeps_a_running_jobs_day_and_deletes_what_no_reader_sees() {
    install();
    let (_root, db, id) = ready();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let jobs = db
        .enqueue_routes(&id, None, "staging", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    db.stage_routes(&id, &job, capture.clone()).unwrap();
    // Its job is still queued: the staged day is being written, not abandoned.
    assert_eq!(sweep(&db, &id), 0);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );
    assert!(db.route_days(&id).unwrap().days.is_empty());
    // A retry starts over: the first attempt's staged day is detached from the job and
    // swept in steps, while the new one, its job still queued, stays.
    db.stage_routes(&id, &job, capture.clone()).unwrap();
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        2
    );
    assert!(sweep(&db, &id) > 1);
    assert_eq!(
        db.routes_db(&id)
            .unwrap()
            .all("SELECT job_id FROM route_publications", [])
            .unwrap(),
        vec![json!({"job_id": job})]
    );
    // Once the job has ended without publishing, the sweep removes every row of it.
    common::set_job_status(&db, &job, "failed").unwrap();
    assert!(sweep(&db, &id) > 1);
    for table in [
        "route_publications",
        "routes",
        "itineraries",
        "stops",
        "tasks",
        "driver_days",
        "breaks",
        "unknown_stops",
        "route_raw",
    ] {
        assert_eq!(
            count(&db, &id, &format!("SELECT count(*) FROM {table}")),
            0,
            "{table}"
        );
    }
}

#[test]
fn a_retention_window_retires_older_days_only_while_routes_are_on() {
    install();
    let (_root, db, id) = ready();
    let actor = common::a_user(&db);
    db.set_feature(&id, "routes", true, &actor).unwrap();
    // Nothing is deleted by default.
    let retention = db.route_retention(&id).unwrap();
    assert_eq!(retention.days, None);
    assert_eq!(retention.stored_days, 0);
    let today = chrono::Utc::now().date_naive();
    let old = (today - chrono::Duration::days(40)).to_string();
    let recent = (today - chrono::Duration::days(1)).to_string();
    for (key, day) in [("old", &old), ("recent", &recent)] {
        let capture = routes::fixture(&request(day, Mode::Final)).unwrap();
        publish(&db, &id, key, day, &capture);
    }
    assert_eq!(db.expire_routes(&id).unwrap(), 0);
    assert_eq!(db.route_retention(&id).unwrap().stored_days, 2);
    // Windows are 30 days to ten years.
    for days in [0, 29, 3651] {
        assert_eq!(
            db.set_route_retention(&id, Some(&actor), Some(days))
                .unwrap_err()
                .code,
            "invalid_retention"
        );
    }
    let retention = db.set_route_retention(&id, Some(&actor), Some(30)).unwrap();
    assert_eq!(retention.days, Some(30));
    assert_eq!(retention.oldest_day.as_deref(), Some(old.as_str()));
    // A day the window has passed cannot be collected again.
    assert_eq!(
        db.enqueue_routes(&id, None, "again", Some(&old), Mode::Final, 1)
            .unwrap_err()
            .code,
        "routes_day_outside_retention"
    );
    // Switched off, routes keep everything; switched on, the old day is retired.
    db.set_feature(&id, "routes", false, &actor).unwrap();
    assert_eq!(db.expire_routes(&id).unwrap(), 0);
    db.set_feature(&id, "routes", true, &actor).unwrap();
    assert_eq!(db.expire_routes(&id).unwrap(), 1);
    let days = db.route_days(&id).unwrap();
    assert_eq!(
        days.days.iter().map(|d| d.day.as_str()).collect::<Vec<_>>(),
        [recent.as_str()]
    );
    assert!(sweep(&db, &id) > 1);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );
    assert_eq!(
        count(
            &db,
            &id,
            &format!("SELECT count(*) FROM tasks WHERE day='{old}'")
        ),
        0
    );
    // Shared rows seen since stay; the recent day still shows them.
    assert_eq!(count(&db, &id, "SELECT count(*) FROM addresses"), 2);
    let actions: Vec<String> = common::audit_actions(&db, &id, "routes.");
    assert!(actions.contains(&"routes.retention_changed".to_owned()));
    assert!(actions.contains(&"routes.data_expired".to_owned()));
    // Keeping every day again deletes nothing more.
    assert_eq!(
        db.set_route_retention(&id, Some(&actor), None)
            .unwrap()
            .days,
        None
    );
    assert_eq!(db.expire_routes(&id).unwrap(), 0);
}

#[test]
fn the_task_key_migration_keeps_every_row_and_compacts_events() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!(
        "../../../migrations/routedata/0001_baseline.sql"
    ))
    .unwrap();
    // Migration 2 as its code applies it: columns first, then its script.
    db.execute_batch(
        "ALTER TABLE tasks ADD COLUMN active INTEGER NOT NULL DEFAULT 1; \
         ALTER TABLE itineraries ADD COLUMN rescue_actions TEXT; \
         ALTER TABLE itineraries ADD COLUMN sequence_edits TEXT; \
         ALTER TABLE itineraries ADD COLUMN pause_events TEXT;",
    )
    .unwrap();
    db.execute_batch(include_str!(
        "../../../migrations/routedata/0002_details.sql"
    ))
    .unwrap();
    db.execute_batch(
        "INSERT INTO route_publications VALUES ('p1','job-1','2026-09-25','TST1','area','provider','UTC','final',\
         '2026-09-26T00:00:00Z','2026-09-26T00:10:00Z',1,1,2,1,1,1); \
         INSERT INTO tasks(publication_id,day,itinerary_id,stop_id,task_id,transporter_id,events,active) VALUES \
         ('p1','2026-09-25','itinerary-2','seq-1','task-1','driver-2',\
          '[{\"executionTime\":10,\"taskState\":\"PICKED_UP\",\"taskStateContext\":\"NONE\"},\
            {\"executionTime\":20,\"taskState\":\"DELIVERED\",\"taskStateContext\":\"DELIVERED_TO_DOORSTEP\"}]',1); \
         INSERT INTO addresses VALUES ('a1','1 Example St',NULL,NULL,'Fixture','CA','90000','','',34.0,-118.0,\
         '2026-09-25','2026-09-25');",
    )
    .unwrap();
    db.execute_batch(include_str!(
        "../../../migrations/routedata/0003_task_keys.sql"
    ))
    .unwrap();
    let events: String = db
        .query_row("SELECT events FROM tasks WHERE task_id='task-1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&events).unwrap(),
        json!([
            [10, "PICKED_UP", "NONE"],
            [20, "DELIVERED", "DELIVERED_TO_DOORSTEP"]
        ])
    );
    // The same task under another itinerary is a row of its own now.
    db.execute(
        "INSERT INTO tasks(publication_id,day,itinerary_id,stop_id,task_id,transporter_id,events,active) \
         VALUES ('p1','2026-09-25','itinerary-1','','task-1','driver-1','[]',0)",
        [],
    )
    .unwrap();
    let rows: i64 = db
        .query_row(
            "SELECT count(*) FROM deliveries WHERE task_id='task-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 2);
    let columns: Vec<String> = db
        .prepare("SELECT name FROM pragma_table_info('addresses')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!columns.iter().any(|c| c.starts_with("customer")));
    let kept: String = db
        .query_row(
            "SELECT address1 FROM addresses WHERE address_id='a1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, "1 Example St");
    // An inactive publication's rows are out of the views.
    db.execute("UPDATE route_publications SET active=0", [])
        .unwrap();
    let rows: i64 = db
        .query_row("SELECT count(*) FROM deliveries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

fn resident(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix(field))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

// A real day through storage, on a private copy of a DSP's state that already holds one
// publication (DISPATCH_BENCHMARK_STATE, DISPATCH_BENCHMARK_DSP_ID, DISPATCH_BENCHMARK_DAY).
// Opening it migrates it; the day is rebuilt from its stored responses, then stored again
// from them the way a job stores a collection, one step per itinerary, published and
// swept. Prints step times and counts, never a value. Run in release, alone.
#[test]
#[ignore = "requires a private copy of a DSP's state"]
fn a_stored_day_goes_through_storage_at_full_size() {
    let root = std::env::var("DISPATCH_BENCHMARK_STATE").expect("DISPATCH_BENCHMARK_STATE");
    let id = std::env::var("DISPATCH_BENCHMARK_DSP_ID").expect("DISPATCH_BENCHMARK_DSP_ID");
    let day = std::env::var("DISPATCH_BENCHMARK_DAY").expect("DISPATCH_BENCHMARK_DAY");
    install();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.into();
    config.environment = "preview".into();
    let started = std::time::Instant::now();
    let db = Store::initialize(config).unwrap();
    let opened_ms = started.elapsed().as_millis();
    let tasks_after_migration = count(&db, &id, "SELECT count(*) FROM tasks");
    let started = std::time::Instant::now();
    let rebuilt = db.reprocess_routes(&id, Some(&day)).unwrap();
    let reprocess_ms = started.elapsed().as_millis();
    let storage = db.routes_db(&id).unwrap();
    let publication = storage
        .one(
            "SELECT id,station,service_area_id,provider,timezone,mode,started_at,collected_at,day \
             FROM route_publications WHERE active=1 AND day=?",
            [&day],
        )
        .unwrap()
        .unwrap();
    // What the stored responses hold, to check the rows against.
    let mut raw_active = 0usize;
    let mut raw_inactive = 0usize;
    let mut raw_delivered = 0i64;
    let bodies: Vec<Vec<u8>> = {
        let mut statement = storage
            .0
            .prepare(
                "SELECT body FROM route_raw WHERE publication_id=? AND name LIKE 'itinerary:%'",
            )
            .unwrap();
        statement
            .query_map([s(&publication, "id")], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    for body in &bodies {
        let value: Value = serde_json::from_slice(&routedata::gunzip(body).unwrap()).unwrap();
        let details = &value["itineraryDetails"];
        let mut ids = std::collections::HashSet::new();
        for stop in details["stops"].as_array().into_iter().flatten() {
            for task in stop["tasks"].as_array().into_iter().flatten() {
                ids.insert(task["taskId"].as_str().unwrap().to_owned());
                if task["taskType"] == "DROP_OFF" && task["taskState"] == "DELIVERED" {
                    raw_delivered += 1;
                }
            }
        }
        raw_active += ids.len();
        raw_inactive += details["inactiveTasks"].as_array().map_or(0, |t| {
            t.iter()
                .map(|t| t["taskId"].as_str().unwrap())
                .collect::<std::collections::HashSet<_>>()
                .len()
        });
    }
    let pid = s(&publication, "id").to_owned();
    let active = count(
        &db,
        &id,
        &format!("SELECT count(*) FROM tasks WHERE publication_id='{pid}' AND active=1"),
    );
    let inactive = count(
        &db,
        &id,
        &format!("SELECT count(*) FROM tasks WHERE publication_id='{pid}' AND active=0"),
    );
    let delivered = count(
        &db,
        &id,
        &format!(
            "SELECT count(*) FROM tasks WHERE publication_id='{pid}' AND active=1 AND task_type='DROP_OFF' AND task_state='DELIVERED'"
        ),
    );
    let summed = count(
        &db,
        &id,
        &format!("SELECT sum(delivered) FROM driver_days WHERE publication_id='{pid}'"),
    );
    assert_eq!(active as usize, raw_active);
    assert_eq!(inactive as usize, raw_inactive);
    assert_eq!(delivered, raw_delivered);
    assert_eq!(summed, raw_delivered);
    // The same day stored again from its responses, as a job would store it.
    let jobs = db
        .enqueue_routes(
            &id,
            None,
            &format!("storage-benchmark-{}", std::process::id()),
            Some(&day),
            Mode::Final,
            1,
        )
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    let mut capture = routes::fixture(
        &routes::Request::parse(
            &serde_json::from_str::<Value>(&db.job_row(&job, Some(&id)).unwrap().request).unwrap(),
        )
        .unwrap()
        .unwrap(),
    )
    .unwrap();
    // The stored responses, not the fixture's: the lists and every itinerary.
    let blob = |name: &str| -> String {
        let body: Vec<u8> = storage
            .0
            .query_row(
                "SELECT body FROM route_raw WHERE publication_id=? AND name=?",
                [pid.as_str(), name],
                |r| r.get(0),
            )
            .unwrap();
        String::from_utf8(routedata::gunzip(&body).unwrap()).unwrap()
    };
    capture.summaries = serde_json::from_str(&blob("summaries")).unwrap();
    capture.route_summaries = serde_json::from_str(&blob("route_summaries")).unwrap();
    capture.scope.service_area_id = s(&publication, "service_area_id").into();
    capture.scope.provider = s(&publication, "provider").into();
    capture.scope.timezone = s(&publication, "timezone").into();
    capture.itineraries = routes::listed(&capture.summaries, &capture.scope)
        .into_iter()
        .map(|(itinerary, transporter)| routes::ItineraryCapture {
            detail: blob(&format!("itinerary:{itinerary}")),
            id: itinerary,
            transporter_id: transporter,
        })
        .collect();
    let before = resident("VmRSS:");
    // As `stage` does it: checking and shaping without the lock, each insert a step.
    let request = routes::Request::parse(
        &serde_json::from_str::<Value>(&db.job_row(&job, Some(&id)).unwrap().request).unwrap(),
    )
    .unwrap()
    .unwrap();
    capture.validate(&request).unwrap();
    let shaper = routedata::Shaper::new(&capture);
    let itineraries = std::mem::take(&mut capture.itineraries);
    let started = std::time::Instant::now();
    let staged = db
        .stage_routes_start(&id, &job, &capture, itineraries.len())
        .unwrap();
    let start_ms = started.elapsed().as_millis();
    let (mut shaping, mut inserts) = (Vec::new(), Vec::new());
    for captured in itineraries {
        let started = std::time::Instant::now();
        let shaped = shaper.shape(&captured).unwrap();
        drop(captured);
        shaping.push(started.elapsed().as_millis());
        let started = std::time::Instant::now();
        db.stage_routes_itinerary(&id, &staged, &shaped).unwrap();
        inserts.push(started.elapsed().as_millis());
    }
    let started = std::time::Instant::now();
    db.publish_routes(&id, &job, &staged).unwrap();
    let publish_ms = started.elapsed().as_millis();
    common::set_job_status(&db, &job, "succeeded").unwrap();
    let mut sweeps = Vec::new();
    loop {
        let started = std::time::Instant::now();
        let more = db.sweep_routes(&id).unwrap();
        sweeps.push(started.elapsed().as_millis());
        if !more {
            break;
        }
    }
    let new_active = count(
        &db,
        &id,
        &format!(
            "SELECT count(*) FROM tasks WHERE publication_id='{}' AND active=1",
            staged.publication
        ),
    );
    assert_eq!(new_active as usize, raw_active);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );
    let file = std::path::Path::new(&std::env::var("DISPATCH_BENCHMARK_STATE").unwrap())
        .join(format!("dsps/{id}/data/routedata/routedata.sqlite"));
    eprintln!(
        "STORAGE {}",
        json!({
            "openedMs": opened_ms, "tasksAfterMigration": tasks_after_migration,
            "reprocessMs": reprocess_ms, "reprocessed": rebuilt.publications.len(),
            "activeTasks": active, "rawActive": raw_active, "inactiveTasks": inactive, "rawInactive": raw_inactive,
            "delivered": delivered, "driverDaysDelivered": summed, "rawDelivered": raw_delivered,
            "stageStartMs": start_ms, "shapeMaxMs": shaping.iter().max(), "shapeTotalMs": shaping.iter().sum::<u128>(),
            "insertMaxMs": inserts.iter().max(), "insertTotalMs": inserts.iter().sum::<u128>(),
            "publishMs": publish_ms,
            "sweepSteps": sweeps.len(), "sweepMaxMs": sweeps.iter().max(), "sweepTotalMs": sweeps.iter().sum::<u128>(),
            "rssBeforeStageKB": before, "hwmKB": resident("VmHWM:"),
            "dbBytes": std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
        })
    );
}
