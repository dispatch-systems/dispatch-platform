//! Routes storage: publication, supersession, the days a schedule queues and what the
//! views read.
use dispatch_core::State;
use dispatch_core::collection::browser::Collected;
use dispatch_core::db::{Store, s};
use dispatch_core::foundation::config::Environment;
use dispatch_core::manifest::{Section, registry};
use dispatch_core::server::startup;
use dispatch_core::settings::{self, Saved};
use dispatch_core::testing as common;
use dispatch_cortex::{
    self as cortex,
    routes::{self, Capture, Mode, Request},
};
use dispatch_routes::{RoutesStore, save_day};
use serde_json::{Value, json};
use std::sync::Arc;

/// Routes, and the Cortex collector whose routes it keeps.
fn install() {
    common::install(&[&cortex::COLLECTOR], &[&dispatch_routes::FEATURE]);
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
    station(&db, &id);
    (root, db, id)
}
/// Gives the DSP a station and an enabled Cortex connection.
fn station(db: &Store, id: &str) {
    db.set_profile(
        id,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(db, id, cortex::PROVIDER).unwrap();
}
/// Leases `job`, the DSP's next, to a worker and stages `capture` for it by Routes' keeper, as the
/// executor does. Answers what the keeper staged: the day's inactive publication.
async fn stage(
    state: &Arc<State>,
    db: &Store,
    id: &str,
    job: &str,
    capture: Capture,
) -> dispatch_core::Result<Collected> {
    let leased = db.claim_job("worker", |dsp, _| dsp == id).unwrap();
    assert_eq!(leased.map(|leased| leased.id).as_deref(), Some(job));
    let collected = Collected {
        data: serde_json::to_value(capture).unwrap(),
        scope: None,
    };
    let keeper = registry().keeper(routes::JOB_KIND);
    keeper.stage(state, id, job, "worker", collected).await
}
/// Publishes what `stage` staged for `job` by Routes' keeper, as the executor does.
fn publish_staged(db: &Store, id: &str, job: &str, staged: Collected) -> dispatch_core::Result<()> {
    registry()
        .keeper(routes::JOB_KIND)
        .publish(db, id, job, staged)
}
/// Queues `day`, stages `capture` for that job and publishes it, as the executor does.
async fn publish(
    state: &Arc<State>,
    db: &Store,
    id: &str,
    key: &str,
    day: &str,
    capture: &Capture,
) -> String {
    let jobs = db
        .enqueue_routes(id, None, key, Some(day), capture.mode, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    finish(state, db, id, &job, capture).await;
    job
}
/// Stages `capture` for `job` and publishes it, as the executor does.
async fn finish(state: &Arc<State>, db: &Store, id: &str, job: &str, capture: &Capture) {
    let staged = stage(state, db, id, job, capture.clone()).await.unwrap();
    publish_staged(db, id, job, staged).unwrap();
    // The executor publishes in the same transaction that finishes the job.
    common::set_job_status(db, job, "succeeded").unwrap();
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
/// Today where the DSP is, as its window counts back from.
fn today(db: &Store, id: &str) -> chrono::NaiveDate {
    let zone: chrono_tz::Tz = db.find_dsp(id).unwrap().timezone.parse().unwrap();
    dispatch_core::foundation::clock::today(zone)
}
/// How long the DSP keeps its route data, as Routes declares it.
fn retention() -> &'static Section {
    let mut sections = dispatch_routes::FEATURE.settings.iter();
    sections.find(|s| s.id == "routes.retention").unwrap()
}

#[tokio::test]
async fn a_day_is_published_into_normalized_rows_with_its_raw_responses() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let job = publish(&state, &db, &id, "first", "2026-09-25", &capture).await;
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
    let inflated: Value = serde_json::from_slice(&dispatch_routes::gunzip(&body).unwrap()).unwrap();
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
    // An old day whose delivered and picked-up task rows were let go reads the same: an
    // itinerary's tasks from its stored response, and a package through the day's lookup.
    let other = db
        .route_itinerary(&id, "2026-09-25", "itinerary-1")
        .unwrap()
        .unwrap();
    // A package only ever removed from a route, whose rows stay.
    let only_removed = db.route_package(&id, "TBA000000000009").unwrap();
    assert!(!only_removed.events.is_empty());
    let storage = db.routes_db(&id).unwrap();
    // The lookup names only what is let go; the undelivered and removed rows stay.
    let removed = "active=1 AND (task_type='PICK_UP' OR task_state='DELIVERED')";
    storage
        .exec(
            &format!(
                "INSERT INTO package_days SELECT DISTINCT tracking_id,publication_id,itinerary_id \
                 FROM tasks WHERE tracking_id IS NOT NULL AND {removed}"
            ),
            [],
        )
        .unwrap();
    let let_go = storage
        .exec(&format!("DELETE FROM tasks WHERE {removed}"), [])
        .unwrap();
    assert!(let_go > 0);
    storage
        .exec("UPDATE route_publications SET tasks_pruned=1", [])
        .unwrap();
    drop(storage);
    for (itinerary, read) in [("itinerary-2", &detail), ("itinerary-1", &other)] {
        assert_eq!(
            db.route_itinerary(&id, "2026-09-25", itinerary)
                .unwrap()
                .as_ref(),
            Some(read),
            "{itinerary}"
        );
    }
    assert_eq!(db.route_package(&id, "TBA000000000002").unwrap(), package);
    assert_eq!(
        db.route_package(&id, "TBA000000000009").unwrap(),
        only_removed
    );
    assert_eq!(db.route_package(&id, "TBA999999999999").unwrap().events, []);
}

#[tokio::test]
async fn a_packages_history_across_many_old_days_reads_the_same_without_their_rows() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    // The package picked up and delivered on two itineraries of each of 125 days, four
    // events a day, and on one itinerary of the newest: the answer's five hundred then end
    // with the two pickups of one day, one on each of its itineraries.
    let newest = chrono::NaiveDate::from_ymd_opt(2026, 9, 25).unwrap();
    for back in 0..126 {
        let day = (newest - chrono::Duration::days(back)).to_string();
        let mut capture = routes::fixture(&request(&day, Mode::Final)).unwrap();
        if back == 0 {
            let first = capture.summaries["itinerarySummaries"][0].clone();
            capture.summaries["itinerarySummaries"] = json!([first]);
            capture.itineraries.truncate(1);
        }
        publish(&state, &db, &id, &format!("day-{back}"), &day, &capture).await;
    }
    let before = db.route_package(&id, "TBA000000000001").unwrap();
    assert_eq!(before.events.len(), 500);
    let last: Vec<_> = (before.events[498..].iter())
        .map(|event| (event.itinerary_id.as_str(), event.task.task_type.as_deref()))
        .collect();
    assert_eq!(
        last,
        [
            ("itinerary-1", Some("PICK_UP")),
            ("itinerary-2", Some("PICK_UP"))
        ]
    );
    let storage = db.routes_db(&id).unwrap();
    let removed = "active=1 AND (task_type='PICK_UP' OR task_state='DELIVERED')";
    storage
        .exec(
            &format!(
                "INSERT INTO package_days SELECT DISTINCT tracking_id,publication_id,itinerary_id \
                 FROM tasks WHERE tracking_id IS NOT NULL AND {removed}"
            ),
            [],
        )
        .unwrap();
    storage
        .exec(&format!("DELETE FROM tasks WHERE {removed}"), [])
        .unwrap();
    storage
        .exec("UPDATE route_publications SET tasks_pruned=1", [])
        .unwrap();
    drop(storage);
    assert_eq!(db.route_package(&id, "TBA000000000001").unwrap(), before);
}

#[tokio::test]
async fn an_old_days_rows_go_only_once_they_read_the_same_from_its_responses() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    // The bootstrapped DSP keeps UTC, so its days are UTC days.
    let today = chrono::Utc::now().date_naive();
    let day = |back: i64| (today - chrono::Duration::days(back)).to_string();
    // Two days older than a month and one inside it.
    let (old, odd, recent) = (day(40), day(41), day(5));
    for (key, day) in [("old", &old), ("odd", &odd), ("recent", &recent)] {
        let capture = routes::fixture(&request(day, Mode::Final)).unwrap();
        publish(&state, &db, &id, key, day, &capture).await;
    }
    let rows = |day: &str, of: &str| {
        count(
            &db,
            &id,
            &format!("SELECT count(*) FROM tasks WHERE day='{day}' AND {of}"),
        )
    };
    let pruned = |day: &str| {
        count(
            &db,
            &id,
            &format!("SELECT tasks_pruned FROM route_publications WHERE day='{day}' AND active=1"),
        )
    };
    let let_go = "active=1 AND (task_type='PICK_UP' OR task_state='DELIVERED')";
    let stays = "NOT (active=1 AND (task_type='PICK_UP' OR task_state='DELIVERED'))";
    let before = (rows(&old, "1"), rows(&odd, "1"), rows(&recent, "1"));
    assert!(rows(&old, let_go) > 0 && rows(&old, stays) > 0);
    let read = |day: &str| {
        (
            db.route_itinerary(&id, day, "itinerary-1").unwrap(),
            db.route_itinerary(&id, day, "itinerary-2").unwrap(),
        )
    };
    let (itineraries, package) = (
        read(&old),
        db.route_package(&id, "TBA000000000002").unwrap(),
    );
    // One task row of the other old day says what its stored response does not.
    db.routes_db(&id)
        .unwrap()
        .exec(
            "UPDATE tasks SET state_context='ODD' WHERE rowid=(SELECT rowid FROM tasks WHERE day=? LIMIT 1)",
            [&odd],
        )
        .unwrap();

    while db.let_go_of_old_route_days(&id).unwrap() {}

    // The day whose every row its responses read the same let its delivered and picked-up
    // rows go, kept the rest, and reads exactly as before.
    assert_eq!(pruned(&old), 1);
    assert_eq!(rows(&old, let_go), 0);
    assert_eq!(rows(&old, "1"), rows(&old, stays));
    assert!(rows(&old, stays) > 0);
    assert!(count(&db, &id, "SELECT count(*) FROM package_days") > 0);
    assert_eq!(read(&old), itineraries);
    assert_eq!(db.route_package(&id, "TBA000000000002").unwrap(), package);
    // The day with a row its response reads otherwise keeps every row, as does a recent one.
    assert_eq!((pruned(&odd), rows(&odd, "1")), (0, before.1));
    assert_eq!((pruned(&recent), rows(&recent, "1")), (0, before.2));
    assert!(before.0 > rows(&old, "1"));
    // Nothing more is due, and a later pass finds the same.
    assert!(!db.let_go_of_old_route_days(&id).unwrap());
}

#[tokio::test]
async fn a_reprocess_rebuilds_every_row_from_the_stored_responses() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    publish(&state, &db, &id, "first", "2026-09-25", &capture).await;
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

/// A prior shaper's staged keys, absent from the current capture. These must never be
/// merged when replay rebuilds that capture's staging.
fn omitted_shared_staging(storage: &dispatch_core::db::Db, publication: &str) {
    storage
        .exec(
            "INSERT INTO staged_addresses(publication_id,address_id,address1) \
             VALUES (?,'address-2','Obsolete street')",
            [publication],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO staged_drivers(publication_id,transporter_id,first_name,person_type) \
             VALUES (?,'driver-2','Obsolete','[]')",
            [publication],
        )
        .unwrap();
}

/// Every persisted row, with raw response bytes compared too (Db's JSON reader omits blobs).
fn replay_storage_rows(storage: &dispatch_core::db::Db) -> Vec<(String, Vec<Value>)> {
    storage
        .all(
            "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
            [],
        )
        .unwrap()
        .into_iter()
        .map(|row| {
            let table = s(&row, "name").to_owned();
            let query = if table == "route_raw" {
                "SELECT publication_id,name,encoding,raw_bytes,hex(body) body FROM route_raw \
                 ORDER BY publication_id,name"
                    .to_owned()
            } else {
                format!(
                    "SELECT * FROM \"{}\" ORDER BY rowid",
                    table.replace('"', "\"\"")
                )
            };
            let rows = storage.all(&query, []).unwrap();
            (table, rows)
        })
        .collect()
}

#[tokio::test]
async fn a_reprocess_rebuilds_shared_staging_without_replaying_omitted_keys() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let day = "2026-09-24";
    let mut capture = routes::fixture(&request(day, Mode::Final)).unwrap();
    capture.itineraries.truncate(1);
    capture.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    // Replaying this day shapes neither address-2 nor a second itinerary's driver.
    let mut detail: Value =
        serde_json::from_str(&capture.itineraries[0].detail.text().unwrap()).unwrap();
    detail["addresses"].as_array_mut().unwrap().truncate(1);
    capture.itineraries[0].detail =
        dispatch_cortex::routes::Detail::of_text(&detail.to_string()).unwrap();
    publish(&state, &db, &id, "replay", day, &capture).await;
    let storage = db.routes_db(&id).unwrap();
    let publication = storage
        .one(
            "SELECT id FROM route_publications WHERE day=? AND active=1",
            [day],
        )
        .unwrap()
        .unwrap();
    omitted_shared_staging(&storage, s(&publication, "id"));

    // A later day's actual publication supplies the identities the omitted keys must keep.
    let mut later = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    later.summaries["transporters"][1]["firstName"] = json!("Current");
    for itinerary in &mut later.itineraries {
        let mut detail: Value = serde_json::from_str(&itinerary.detail.text().unwrap()).unwrap();
        detail["addresses"][1]["address1"] = json!("Current street");
        itinerary.detail = dispatch_cortex::routes::Detail::of_text(&detail.to_string()).unwrap();
    }
    publish(&state, &db, &id, "later", "2026-09-25", &later).await;

    // An unfinished publication, cancelled before activation, owns separate staging.
    let jobs = db
        .enqueue_routes(&id, None, "cancelled", Some(day), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    let cancelled = stage(
        &state,
        &db,
        &id,
        job,
        renamed(
            &routes::fixture(&request(day, Mode::Final)).unwrap(),
            "Cancelled",
            "Cancelled street",
        ),
    )
    .await
    .unwrap();
    common::set_job_status(&db, job, "cancelled").unwrap();
    let staged = |table: &str| {
        storage
            .all(
                &format!("SELECT * FROM {table} WHERE publication_id=? ORDER BY rowid"),
                [s(&cancelled.data, "publication")],
            )
            .unwrap()
    };
    let unconsumed = (staged("staged_addresses"), staged("staged_drivers"));
    assert!(!unconsumed.0.is_empty());
    assert!(!unconsumed.1.is_empty());

    // Another DSP uses the same driver/address keys. Replaying this DSP changes none of it.
    let owner = common::platform_owner(&db);
    let other = db
        .new_dsp("Other fixture", "UTC", &owner, false)
        .unwrap()
        .id;
    station(&db, &other);
    db.set_feature(&other, "routes", true, &owner).unwrap();
    let other_storage = db.routes_db(&other).unwrap();
    let jobs = db
        .enqueue_routes(&other, None, "other", Some(day), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    let other_staged = stage(&state, &db, &other, job, capture.clone())
        .await
        .unwrap();
    omitted_shared_staging(&other_storage, s(&other_staged.data, "publication"));
    let other_before = replay_storage_rows(&other_storage);
    let raw_before = storage
        .all(
            "SELECT publication_id,name,encoding,raw_bytes,hex(body) body FROM route_raw \
         ORDER BY publication_id,name",
            [],
        )
        .unwrap();

    for _ in 0..2 {
        let replayed = db.reprocess_routes(&id, Some(day)).unwrap();
        assert_eq!(replayed.publications.len(), 1);
        assert_eq!(replayed.publications[0].itinerary_count, 1);
        assert_eq!(storage.one(
            "SELECT (SELECT first_name FROM drivers WHERE transporter_id='driver-2') driver,\
             (SELECT address1 FROM addresses WHERE address_id='address-2') street", [],
        ).unwrap(), Some(json!({"driver":"Current","street":"Current street"})));
        assert_eq!(
            (staged("staged_addresses"), staged("staged_drivers")),
            unconsumed
        );
        for table in ["staged_addresses", "staged_drivers"] {
            assert_eq!(
                storage
                    .count(
                        &format!("SELECT count(*) FROM {table} WHERE publication_id=?"),
                        [s(&publication, "id")],
                    )
                    .unwrap(),
                0
            );
        }
        assert_eq!(replay_storage_rows(&other_storage), other_before);
        assert_eq!(
            storage
                .all(
                    "SELECT publication_id,name,encoding,raw_bytes,hex(body) body FROM route_raw \
             ORDER BY publication_id,name",
                    [],
                )
                .unwrap(),
            raw_before
        );
    }
    assert_eq!(db.route_days(&id).unwrap().days.len(), 2);
    assert_eq!(
        storage
            .count(
                "SELECT count(*) FROM route_publications WHERE id=? AND active=0",
                [s(&cancelled.data, "publication")]
            )
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_failed_reprocess_restores_rows_and_staging_and_needs_only_inline_responses() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let day = "2026-09-25";
    let capture = routes::fixture(&request(day, Mode::Final)).unwrap();
    publish(&state, &db, &id, "replay", day, &capture).await;
    let storage = db.routes_db(&id).unwrap();
    let publication = storage
        .one("SELECT id FROM route_publications WHERE active=1", [])
        .unwrap()
        .unwrap();
    // Extra keys model staging a previous reader left, including one no raw response names.
    storage.exec(
        "INSERT INTO staged_addresses(publication_id,address_id,address1) VALUES (?,'omitted','Old')",
        [s(&publication, "id")],
    ).unwrap();
    storage
        .exec(
            "INSERT INTO staged_drivers(publication_id,transporter_id,first_name,person_type) \
         VALUES (?,'omitted','Old','[]')",
            [s(&publication, "id")],
        )
        .unwrap();
    let before = replay_storage_rows(&storage);
    // Fail after the fresh staging was shared, but before the day transaction commits.
    storage.0.execute_batch(
        "CREATE TEMP TRIGGER refuse_reprocess BEFORE UPDATE OF adapter_version ON route_publications \
         BEGIN SELECT RAISE(ABORT,'fixture reprocess refused'); END;",
    ).unwrap();
    let error = db.reprocess_routes(&id, Some(day)).unwrap_err();
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .to_string()
            .contains("fixture reprocess refused")
    );
    assert_eq!(replay_storage_rows(&storage), before);
    storage
        .0
        .execute_batch("DROP TRIGGER refuse_reprocess")
        .unwrap();

    // Corrupt and missing legacy inline responses refuse before any rows or staging change.
    let raw: (i64, Vec<u8>) = storage.0.query_row(
        "SELECT raw_bytes,body FROM route_raw WHERE publication_id=? AND name='itinerary:itinerary-1'",
        [s(&publication, "id")], |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    storage.exec(
        "UPDATE route_raw SET body=x'00' WHERE publication_id=? AND name='itinerary:itinerary-1'",
        [s(&publication, "id")],
    ).unwrap();
    let corrupt = replay_storage_rows(&storage);
    assert_eq!(
        db.reprocess_routes(&id, Some(day)).unwrap_err().code,
        "operation_failed"
    );
    assert_eq!(replay_storage_rows(&storage), corrupt);
    storage
        .exec(
            "DELETE FROM route_raw WHERE publication_id=? AND name='itinerary:itinerary-1'",
            [s(&publication, "id")],
        )
        .unwrap();
    let missing = replay_storage_rows(&storage);
    assert_eq!(
        db.reprocess_routes(&id, Some(day)).unwrap_err().code,
        "operation_failed"
    );
    assert_eq!(replay_storage_rows(&storage), missing);
    storage
        .exec(
            "INSERT INTO route_raw(publication_id,name,encoding,raw_bytes,body) \
         VALUES (?,'itinerary:itinerary-1','gzip',?,?)",
            rusqlite::params![s(&publication, "id"), raw.0, raw.1],
        )
        .unwrap();
    assert_eq!(replay_storage_rows(&storage), before);
    // Restored inline bytes are sufficient: no format, sidecar or metadata migration is needed.
    assert_eq!(
        db.reprocess_routes(&id, Some(day))
            .unwrap()
            .publications
            .len(),
        1
    );
    assert_eq!(
        db.route_day(&id, day).unwrap().unwrap().itineraries.len(),
        2
    );
    for table in ["staged_addresses", "staged_drivers"] {
        assert_eq!(
            storage
                .count(&format!("SELECT count(*) FROM {table}"), [])
                .unwrap(),
            0
        );
    }
    assert!(
        storage
            .all("PRAGMA foreign_key_check", [])
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn mismatched_incomplete_and_oversized_captures_fail_before_changing_publications() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    // An itinerary that is not the one listed is refused before anything is stored.
    let mut swapped = capture.clone();
    swapped.itineraries.swap(0, 1);
    let jobs = db
        .enqueue_routes(&id, None, "swapped", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    let error = stage(&state, &db, &id, job, swapped)
        .await
        .map(drop)
        .unwrap_err();
    assert_eq!(error.code, "routes_scope_mismatch");
    common::set_job_status(&db, job, "failed").unwrap();
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        0
    );
    // A staged day missing an itinerary is never made active.
    let jobs = db
        .enqueue_routes(&id, None, "partial", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    let staged = stage(&state, &db, &id, job, capture.clone()).await.unwrap();
    db.routes_db(&id)
        .unwrap()
        .exec(
            "DELETE FROM itineraries WHERE itinerary_id='itinerary-2'",
            [],
        )
        .unwrap();
    let error = publish_staged(&db, &id, job, staged).unwrap_err();
    assert_eq!(error.code, "routes_capture_invalid");
    common::set_job_status(&db, job, "failed").unwrap();
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM route_publications WHERE active=1"
        ),
        0
    );
    assert!(db.route_days(&id).unwrap().days.is_empty());

    publish(&state, &db, &id, "ordinary", "2026-09-25", &capture).await;
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

#[tokio::test]
async fn a_package_moved_between_drivers_keeps_a_row_under_each() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let mut capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    // The second driver delivered task-13; the first driver's itinerary lists it as
    // removed from their route, as Amazon lists a rescued package.
    let second: Value =
        serde_json::from_str(&capture.itineraries[1].detail.text().unwrap()).unwrap();
    let rescued = second["itineraryDetails"]["stops"][1]["tasks"][0].clone();
    assert_eq!(rescued["taskId"], "task-13");
    let mut first: Value =
        serde_json::from_str(&capture.itineraries[0].detail.text().unwrap()).unwrap();
    first["itineraryDetails"]["inactiveTasks"]
        .as_array_mut()
        .unwrap()
        .push(rescued);
    capture.itineraries[0].detail =
        dispatch_cortex::routes::Detail::of_text(&first.to_string()).unwrap();
    publish(&state, &db, &id, "rescue", "2026-09-25", &capture).await;
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

/// `capture` naming its first driver and first address anew, as a later read might.
fn renamed(capture: &Capture, name: &str, street: &str) -> Capture {
    let mut renamed = capture.clone();
    renamed.summaries["transporters"][0]["firstName"] = json!(name);
    for itinerary in &mut renamed.itineraries {
        let mut detail: Value = serde_json::from_str(&itinerary.detail.text().unwrap()).unwrap();
        detail["addresses"][0]["address1"] = json!(street);
        itinerary.detail = dispatch_cortex::routes::Detail::of_text(&detail.to_string()).unwrap();
    }
    renamed
}
#[tokio::test]
async fn a_later_read_of_a_day_replaces_an_earlier_one_in_either_order_and_keeps_shared_rows() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let first = publish(&state, &db, &id, "first", "2026-09-25", &capture).await;
    let mut again = renamed(&capture, "Newer", "1 Newer St");
    again.itineraries.truncate(1);
    again.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    again.started_at += 1000;
    again.finished_at += 2000;
    let second = publish(&state, &db, &id, "second", "2026-09-25", &again).await;
    assert_ne!(first, second);
    // A read collected before the day's own changes nothing, though it arrives after it: not
    // the day, nor the drivers and addresses the days share. Nor does one collected later
    // that never becomes the day's read, its job ended before publishing.
    let mut earlier = renamed(&capture, "Older", "1 Older St");
    earlier.finished_at += 1000;
    publish(&state, &db, &id, "earlier", "2026-09-25", &earlier).await;
    let mut unfinished = renamed(&capture, "Unfinished", "1 Unfinished St");
    unfinished.finished_at += 3000;
    let jobs = db
        .enqueue_routes(&id, None, "unfinished", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id");
    stage(&state, &db, &id, job, unfinished).await.unwrap();
    common::set_job_status(&db, job, "failed").unwrap();
    let storage = db.routes_db(&id).unwrap();
    let shared = storage
        .all(
            "SELECT (SELECT first_name FROM drivers WHERE transporter_id='driver-1') driver,\
             (SELECT address1 FROM addresses WHERE address_id='address-1') street",
            [],
        )
        .unwrap();
    assert_eq!(
        shared,
        vec![json!({"driver":"Newer","street":"1 Newer St"})]
    );
    // Readers see only the later read at once; the sweep deletes the others in steps.
    let days = db.route_days(&id).unwrap();
    assert_eq!(days.days.len(), 1);
    assert_eq!(days.days[0].itinerary_count, 1);
    assert_eq!(
        count(
            &db,
            &id,
            "SELECT count(*) FROM route_publications WHERE active=0"
        ),
        3
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
    let job = s(&jobs[0], "id");
    let refused = stage(&state, &db, &id, job, other)
        .await
        .map(drop)
        .unwrap_err();
    assert_eq!(refused.code, "routes_scope_mismatch");
}

#[tokio::test]
async fn another_features_read_of_a_day_is_saved_whole_when_read_later() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let day = "2026-09-25";
    let capture = routes::fixture(&request(day, Mode::Final)).unwrap();
    let hour = 3_600_000;
    // The day's read, by its itineraries.
    let read = |db: &Store| {
        let days = db.route_days(&id).unwrap().days;
        days.iter().map(|d| d.itinerary_count).collect::<Vec<_>>()
    };
    let save = |key: &'static str, capture: &Capture| save_day(&state, &id, key, capture.clone());

    // Saved whole; saved again under its key, nothing changes.
    assert!(save("live-1", &capture).await.unwrap());
    assert_eq!(read(&db), [2]);
    assert!(save("live-1", &capture).await.unwrap());
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );

    // The standard collection's read, collected later, replaces it whole.
    let mut later = capture.clone();
    later.itineraries.truncate(1);
    later.summaries["itinerarySummaries"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    later.finished_at += 2000;
    publish(&state, &db, &id, "standard", day, &later).await;
    assert_eq!(read(&db), [1]);
    // One collected before it changes nothing; one collected after replaces it.
    let mut earlier = capture.clone();
    earlier.finished_at += 1000;
    assert!(!save("live-2", &earlier).await.unwrap());
    assert_eq!(read(&db), [1]);
    let mut latest = capture.clone();
    latest.finished_at += 3000;
    assert!(save("live-3", &latest).await.unwrap());
    assert_eq!(read(&db), [2]);
    sweep(&db, &id);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );

    // Refused, each for one thing, before anything is stored.
    let mut missing = latest.clone();
    missing.finished_at += 1000;
    missing.itineraries.pop();
    let mut elsewhere = latest.clone();
    elsewhere.scope.service_area_id = "area-other".into();
    let mut another = latest.clone();
    another.scope.provider = "provider-other".into();
    let mut zoned = latest.clone();
    zoned.scope.timezone = "America/Chicago".into();
    // A day's final read once it ended, its snapshot while it lasts.
    let mut unfinished = latest.clone();
    unfinished.finished_at = format!("{day}T20:00:00Z")
        .parse::<chrono::DateTime<chrono::Utc>>()
        .unwrap()
        .timestamp_millis();
    unfinished.started_at = unfinished.finished_at - hour;
    let mut past = latest.clone();
    past.mode = Mode::Snapshot;
    for (key, capture, code) in [
        ("live-4", &unfinished, "routes_day_not_available"),
        ("live-4", &past, "routes_day_not_available"),
        ("live-4", &missing, "routes_capture_invalid"),
        ("live-4", &elsewhere, "routes_scope_mismatch"),
        ("live-4", &another, "routes_scope_mismatch"),
        ("live-4", &zoned, "routes_scope_mismatch"),
        ("live 4", &latest, "invalid_input"),
    ] {
        let refused = save(key, capture).await.unwrap_err();
        assert_eq!(refused.code, code, "{key}");
    }
    let actor = common::a_user(&db);
    db.set_feature(&id, "routes", false, &actor).unwrap();
    let refused = save("live-4", &latest).await.unwrap_err();
    assert_eq!(refused.code, "routes_off");
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );
}

#[tokio::test]
async fn a_schedule_queues_recent_days_without_a_final_publication() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    // The bootstrapped DSP keeps UTC, so its days are UTC days.
    let today = chrono::Utc::now().date_naive();
    let day = |back: i64| (today - chrono::Duration::days(back)).to_string();
    let jobs = db.routes_jobs(&id, Mode::Final).unwrap();
    // Yesterday first, then the days before, at most four per run.
    assert_eq!(jobs.len(), dispatch_routes::MAX_JOBS_PER_RUN);
    assert_eq!(jobs[0].0, format!("routes:{}", day(1)));
    assert_eq!(jobs[0].1["date"], day(1));
    assert_eq!(jobs[0].1["mode"], "final");
    assert_eq!(jobs[0].1["station"], "TST1");
    assert_eq!(jobs[3].0, format!("routes:{}", day(4)));
    // A published day is not asked for again.
    let capture = routes::fixture(&request(&day(1), Mode::Final)).unwrap();
    publish(&state, &db, &id, "yesterday", &day(1), &capture).await;
    let jobs = db.routes_jobs(&id, Mode::Final).unwrap();
    assert_eq!(jobs[0].0, format!("routes:{}", day(2)));
    assert!(
        jobs.iter()
            .all(|(key, _)| key != &format!("routes:{}", day(1)))
    );
    // A snapshot of today is a different reading and leaves the schedule's view alone.
    let snapshot = routes::fixture(&request(&day(0), Mode::Snapshot)).unwrap();
    publish(&state, &db, &id, "today", &day(0), &snapshot).await;
    assert_eq!(db.route_days(&id).unwrap().days.len(), 2);
    assert_eq!(
        db.routes_jobs(&id, Mode::Final).unwrap()[0].0,
        format!("routes:{}", day(2))
    );
}

#[tokio::test]
async fn a_schedule_of_today_reads_it_so_far_and_leaves_its_final_read_to_the_next_day() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    db.enable_all_features(&id).unwrap();
    // The bootstrapped DSP keeps UTC, so its days are UTC days.
    let today = chrono::Utc::now().date_naive();
    let day = |back: i64| (today - chrono::Duration::days(back)).to_string();
    // A DSP's daily schedule of `collection`, run once as it falls due: the requests it
    // queued, newest day first.
    let run = |collection: &str| -> Vec<Value> {
        let rule = json!({"collects":collection,"kind":"times","days":[0,1,2,3,4,5,6],
            "times":["09:00"]});
        let saved = common::schedule(&db, &id, "routes", json!([rule])).unwrap();
        let key = saved.rules[0].id.clone().unwrap();
        common::schedule_due_at(&db, &id, &key, "2026-01-01T00:00:00.000Z").unwrap();
        db.schedule_due(&id).unwrap();
        let mut queued: Vec<Value> = db
            .active_job_ids(&id, routes::JOB_KIND)
            .unwrap()
            .iter()
            .map(|job| serde_json::from_str(&db.job_row(job, Some(&id)).unwrap().request).unwrap())
            .collect();
        queued.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
        common::set_every_status(&db, "succeeded").unwrap();
        queued
    };
    let read = |request: &Value| (request["mode"].clone(), request["date"].clone());

    // Today so far: today alone, read as it stands.
    let queued = run("routes_today");
    assert_eq!(queued.len(), 1);
    assert_eq!(read(&queued[0]), (json!("snapshot"), json!(day(0))));

    // Yesterday, read so far while it was today, is still read whole by a schedule of
    // yesterday.
    let so_far = serde_json::to_value(request(&day(1), Mode::Snapshot)).unwrap();
    let jobs = db
        .enqueue_batch(&id, None, &[("so-far".into(), cortex::PROVIDER, so_far)])
        .unwrap();
    let capture = routes::fixture(&request(&day(1), Mode::Snapshot)).unwrap();
    finish(&state, &db, &id, &jobs[0].id, &capture).await;
    assert_eq!(db.route_days(&id).unwrap().days[0].mode, "snapshot");
    let queued = run("routes");
    assert_eq!(read(&queued[0]), (json!("final"), json!(day(1))));
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

#[tokio::test]
async fn the_sweep_keeps_a_running_jobs_day_and_deletes_what_no_reader_sees() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let capture = routes::fixture(&request("2026-09-25", Mode::Final)).unwrap();
    let jobs = db
        .enqueue_routes(&id, None, "staging", Some("2026-09-25"), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    stage(&state, &db, &id, &job, capture.clone())
        .await
        .unwrap();
    // Its job still runs: the staged day is being written, not abandoned.
    assert_eq!(sweep(&db, &id), 0);
    assert_eq!(
        count(&db, &id, "SELECT count(*) FROM route_publications"),
        1
    );
    assert!(db.route_days(&id).unwrap().days.is_empty());
    // A retry starts over: the first attempt's staged day is detached from the job and
    // swept in steps, while the new one, its job running again, stays.
    common::set_job_status(&db, &job, "queued").unwrap();
    stage(&state, &db, &id, &job, capture.clone())
        .await
        .unwrap();
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
        "staged_addresses",
        "staged_drivers",
    ] {
        assert_eq!(
            count(&db, &id, &format!("SELECT count(*) FROM {table}")),
            0,
            "{table}"
        );
    }
}

#[tokio::test]
async fn a_retention_window_retires_older_days_only_while_routes_are_on() {
    install();
    let (_root, db, id) = ready();
    let state = State::new(db.config.clone()).unwrap();
    let actor = common::a_user(&db);
    db.set_feature(&id, "routes", true, &actor).unwrap();
    let today = today(&db, &id);
    let old = (today - chrono::Duration::days(40)).to_string();
    let recent = (today - chrono::Duration::days(1)).to_string();
    for (key, day) in [("old", &old), ("recent", &recent)] {
        let capture = routes::fixture(&request(day, Mode::Final)).unwrap();
        publish(&state, &db, &id, key, day, &capture).await;
    }
    // Nothing is deleted by default.
    assert_eq!(db.expire_routes(&id).unwrap(), 0);
    // A window that passes a stored day asks first; the day its words name is the settings
    // module's own test, either side of midnight.
    let window = json!({"days": 30});
    assert!(matches!(
        settings::save(&db, &id, &actor, retention(), &window, 0, false).unwrap(),
        Saved::Confirm(_)
    ));
    assert_eq!(db.expire_routes(&id).unwrap(), 0);
    settings::save(&db, &id, &actor, retention(), &window, 0, true).unwrap();
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
    assert!(actions.contains(&"routes.data_expired".to_owned()));
    // Keeping every day again deletes nothing more, so it asks nothing.
    let every = json!({"days": null});
    assert!(matches!(
        settings::save(&db, &id, &actor, retention(), &every, 1, false).unwrap(),
        Saved::Saved { revision: 2, .. }
    ));
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
#[tokio::test]
#[ignore = "requires a private copy of a DSP's state"]
async fn a_stored_day_goes_through_storage_at_full_size() {
    let root = std::env::var("DISPATCH_BENCHMARK_STATE").expect("DISPATCH_BENCHMARK_STATE");
    let id = std::env::var("DISPATCH_BENCHMARK_DSP_ID").expect("DISPATCH_BENCHMARK_DSP_ID");
    let day = std::env::var("DISPATCH_BENCHMARK_DAY").expect("DISPATCH_BENCHMARK_DAY");
    install();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.into();
    config.environment = Environment::Preview;
    let started = std::time::Instant::now();
    let db = startup::prepare_storage(config).unwrap();
    let opened_ms = started.elapsed().as_millis();
    let state = State::new(db.config.clone()).unwrap();
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
        let value: Value = serde_json::from_slice(&dispatch_routes::gunzip(body).unwrap()).unwrap();
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
        String::from_utf8(dispatch_routes::gunzip(&body).unwrap()).unwrap()
    };
    capture.summaries = serde_json::from_str(&blob("summaries")).unwrap();
    capture.route_summaries = serde_json::from_str(&blob("route_summaries")).unwrap();
    capture.scope.service_area_id = s(&publication, "service_area_id").into();
    capture.scope.provider = s(&publication, "provider").into();
    capture.scope.timezone = s(&publication, "timezone").into();
    capture.itineraries = routes::listed(&capture.summaries, &capture.scope)
        .into_iter()
        .map(|(itinerary, transporter)| routes::ItineraryCapture {
            detail: dispatch_cortex::routes::Detail::of_text(&blob(&format!(
                "itinerary:{itinerary}"
            )))
            .unwrap(),
            id: itinerary,
            transporter_id: transporter,
        })
        .collect();
    let before = resident("VmRSS:");
    let started = std::time::Instant::now();
    let staged = stage(&state, &db, &id, &job, capture).await.unwrap();
    let stage_ms = started.elapsed().as_millis();
    let staged_publication = s(&staged.data, "publication").to_owned();
    let started = std::time::Instant::now();
    publish_staged(&db, &id, &job, staged).unwrap();
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
            staged_publication
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
            "stageMs": stage_ms,
            "publishMs": publish_ms,
            "sweepSteps": sweeps.len(), "sweepMaxMs": sweeps.iter().max(), "sweepTotalMs": sweeps.iter().sum::<u128>(),
            "rssBeforeStageKB": before, "hwmKB": resident("VmHWM:"),
            "dbBytes": std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0),
        })
    );
}
