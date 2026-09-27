//! Routes storage: publication, supersession, the days a schedule queues and what the
//! views read.
mod common;
use dispatch_backend::{
    collectors::Provider,
    db::{Store, s},
    itineraries::{self, Capture, Mode, Request},
};
use serde_json::{Value, json};

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
    db.collector(&id, Provider::Cortex)
        .unwrap()
        .exec("UPDATE connections SET enabled=1,status='ready'", [])
        .unwrap();
    (root, db, id)
}
/// Queues `day` and publishes `capture` as that job's outcome.
fn publish(db: &Store, id: &str, key: &str, day: &str, capture: &Capture) -> String {
    let jobs = db
        .enqueue_routes(id, None, key, Some(day), capture.mode, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    db.publish_routes(id, &job, capture).unwrap();
    job
}
fn count(db: &Store, id: &str, sql: &str) -> i64 {
    db.itineraries(id).unwrap().count(sql, []).unwrap()
}

#[test]
fn a_day_is_published_into_normalized_rows_with_its_raw_responses() {
    let (_root, db, id) = ready();
    let capture = itineraries::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    // Prepared ahead, as the executor does outside the platform lock.
    let prepared = itineraries::prepare(capture.clone()).unwrap();
    assert!(prepared.prepared.is_some());
    assert!(prepared.itineraries.iter().all(|i| i.detail.is_empty()));
    let job = publish(&db, &id, "first", "2026-09-25", &prepared);
    let queued: Value = db.job(&job, Some(&id)).unwrap();
    assert_eq!(queued["kind"], "cortex.routes.collect");
    let storage = db.itineraries(&id).unwrap();
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
             WHERE publication_id=? AND itinerary_id='itinerary-2' ORDER BY task_id",
            [pid],
        )
        .unwrap();
    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks[0]["tracking_id"], "TBA000000000001");
    assert_eq!(tasks[0]["transporter_id"], "driver-2");
    assert_eq!(tasks[3]["task_state"], "UNDELIVERABLE");
    assert_eq!(tasks[3]["latitude"], 34.01);
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
    let inflated: Value = serde_json::from_slice(&itineraries::gunzip(&body).unwrap()).unwrap();
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
}

#[test]
fn a_recollected_day_replaces_its_previous_publication_and_keeps_shared_rows() {
    let (_root, db, id) = ready();
    let capture = itineraries::fixture(&request("2026-09-25", Mode::Final)).unwrap();
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
    let storage = db.itineraries(&id).unwrap();
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
    assert_eq!(count(&db, &id, "SELECT count(*) FROM tasks"), 4);
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
    let other = itineraries::fixture(&request("2026-09-24", Mode::Final)).unwrap();
    let jobs = db
        .enqueue_routes(&id, None, "third", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    assert!(db.publish_routes(&id, s(&jobs[0], "id"), &other).is_err());
}

#[test]
fn a_schedule_queues_recent_days_without_a_final_publication() {
    let (_root, db, id) = ready();
    // The bootstrapped DSP keeps UTC, so its days are UTC days.
    let today = chrono::Utc::now().date_naive();
    let day = |back: i64| (today - chrono::Duration::days(back)).to_string();
    let jobs = db.routes_jobs(&id).unwrap();
    // Yesterday first, then the days before, at most four per run.
    assert_eq!(jobs.len(), itineraries::MAX_JOBS_PER_RUN);
    assert_eq!(jobs[0].0, format!("routes:{}", day(1)));
    assert_eq!(jobs[0].1["date"], day(1));
    assert_eq!(jobs[0].1["mode"], "final");
    assert_eq!(jobs[0].1["station"], "TST1");
    assert_eq!(jobs[3].0, format!("routes:{}", day(4)));
    // A published day is not asked for again.
    let capture = itineraries::fixture(&request(&day(1), Mode::Final)).unwrap();
    publish(&db, &id, "yesterday", &day(1), &capture);
    let jobs = db.routes_jobs(&id).unwrap();
    assert_eq!(jobs[0].0, format!("routes:{}", day(2)));
    assert!(
        jobs.iter()
            .all(|(key, _)| key != &format!("routes:{}", day(1)))
    );
    // A snapshot of today is a different reading and leaves the schedule's view alone.
    let snapshot = itineraries::fixture(&request(&day(0), Mode::Snapshot)).unwrap();
    publish(&db, &id, "today", &day(0), &snapshot);
    assert_eq!(db.route_days(&id).unwrap().days.len(), 2);
    assert_eq!(
        db.routes_jobs(&id).unwrap()[0].0,
        format!("routes:{}", day(2))
    );
}

#[test]
fn collection_requests_need_a_station_and_an_allowed_day() {
    let (_root, db, id) = common::bootstrapped();
    db.collector(&id, Provider::Cortex)
        .unwrap()
        .exec("UPDATE connections SET enabled=1,status='ready'", [])
        .unwrap();
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
