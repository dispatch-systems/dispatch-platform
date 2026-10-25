//! Live route data: today's whole reads kept in place, read by read, which must read as each read
//! staged whole into historical route data does; what a read that isn't whole changes, which is
//! nothing; work phones settled across a restart, and who held them read as other features read
//! it; the day saved into historical route data and live route data emptied; and Driver Match
//! told of the drivers it names, and other features of who holds a work phone. Made-up days only.
use dispatch_core::{
    State,
    db::{Db, Store, s},
    manifest::{Feature, Upkeep, events::PeopleChanged, feature, optional},
    testing as common,
};
use dispatch_cortex::{
    self as cortex,
    routes::{self, Capture, Mode, Request},
};
use dispatch_routes::{LiveKept, RoutesStore, clear_live, save_and_clear_live, save_live_day};
use dispatch_routes_contract::{WorkPhoneHold, WorkPhonesChanged};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const P1: &str = "5550100001";
const P2: &str = "5550100002";
const P3: &str = "5550100003";

/// A feature hearing of the people the DSP's data names changing, and of who holds its work
/// phones, on for a test's DSP as every feature is: Driver Match, and texting, as Routes tells
/// them.
static HEARING: Feature = Feature {
    switch: optional("hearing", "Hearing", &[]),
    on: &[
        dispatch_core::on!(PeopleChanged, people_changed),
        dispatch_core::on!(WorkPhonesChanged, phones_changed),
    ],
    ..feature("hearing")
};
/// Each DSP whose people the stand-in heard changed, once each time.
static CHANGED: Mutex<Vec<String>> = Mutex::new(Vec::new());
fn people_changed(_: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    let mut heard = CHANGED.lock().unwrap_or_else(|poison| poison.into_inner());
    heard.push(dsp);
    Box::pin(async {})
}
/// How many times the stand-in heard the DSP's people changed.
fn changed(dsp: &str) -> usize {
    let heard = CHANGED.lock().unwrap_or_else(|poison| poison.into_inner());
    heard.iter().filter(|at| *at == dsp).count()
}
/// Each time the stand-in heard who holds a DSP's work phones changed, the DSP and what it read
/// then: how many holds live route data has, and historical route data keeps of today.
static PHONES: Mutex<Vec<(String, usize, usize)>> = Mutex::new(Vec::new());
fn phones_changed(state: Arc<State>, dsp: String, _: WorkPhonesChanged) -> Upkeep {
    Box::pin(async move {
        let d = dsp.clone();
        let read = state.read(move |db| {
            let today = today(db, &d).to_string();
            Ok((live_holds(db, &d).len(), saved_holds(db, &d, &today).len()))
        });
        let (live, saved) = read.await.unwrap();
        let mut heard = PHONES.lock().unwrap_or_else(|poison| poison.into_inner());
        heard.push((dsp, live, saved));
    })
}
/// What the stand-in read each time it heard who holds the DSP's work phones changed.
fn phones_heard(dsp: &str) -> Vec<(usize, usize)> {
    let heard = PHONES.lock().unwrap_or_else(|poison| poison.into_inner());
    let of = heard.iter().filter(|(at, ..)| at == dsp);
    of.map(|(_, live, saved)| (*live, *saved)).collect()
}

/// A DSP with a station and Cortex connected, Routes on, with its server's state.
fn ready() -> (tempfile::TempDir, Store, String, Arc<State>) {
    common::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_routes::FEATURE, &HEARING],
    );
    let (root, db, dsp) = common::bootstrapped();
    db.set_profile(
        &dsp,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &dsp, cortex::PROVIDER).unwrap();
    let state = State::new(db.config.clone()).unwrap();
    (root, db, dsp, state)
}
/// Keeps `read` as a live reader does, under the DSP's Cortex connection as it stands.
async fn keep_live(
    state: &Arc<State>,
    dsp: &str,
    read: Capture,
) -> dispatch_core::Result<LiveKept> {
    let d = dsp.to_owned();
    let revision = state
        .read(move |db| db.connection_revision(&d, cortex::PROVIDER))
        .await?
        .expect("Cortex is connected");
    dispatch_routes::keep_live(state, dsp, read, revision).await
}
/// Today where the DSP is.
fn today(db: &Store, dsp: &str) -> chrono::NaiveDate {
    let zone: chrono_tz::Tz = db.find_dsp(dsp).unwrap().timezone.parse().unwrap();
    dispatch_core::foundation::clock::today(zone)
}

/// A made-up day of two drivers, each on a work phone of their own and out since the day's
/// first read, which a test changes read by read the ways a day changes.
struct Day {
    summaries: Value,
    route_summaries: Value,
    /// Each itinerary's response, by its id.
    details: BTreeMap<String, Value>,
    lists: Capture,
}
impl Day {
    fn new(day: &str) -> Self {
        let request = Request::parse(&json!({"collection":"routes","mode":"snapshot","date":day,
            "station":"TST1","timezone":"UTC","dspName":"Test Owner","dspAbbreviation":"NLOG"}))
        .unwrap()
        .unwrap();
        let mut lists = routes::fixture(&request).unwrap();
        let details = std::mem::take(&mut lists.itineraries)
            .into_iter()
            .map(|i| (i.id, serde_json::from_str(&i.detail).unwrap()))
            .collect();
        let mut day = Self {
            summaries: lists.summaries.clone(),
            route_summaries: lists.route_summaries.clone(),
            details,
            lists,
        };
        for (n, phone) in [P1, P2].into_iter().enumerate() {
            day.phone(n, phone);
            day.summaries["itinerarySummaries"][n]["lastDriverEventTime"] =
                json!(day.lists.started_at);
        }
        day
    }
    /// The `n`th driver's work phone, as every list and their itinerary show it.
    fn phone(&mut self, n: usize, phone: &str) {
        self.summaries["transporters"][n]["workPhoneNumber"] = json!(phone);
        self.route_summaries["transporters"][n]["workPhoneNumber"] = json!(phone);
        let itinerary = format!("itinerary-{}", n + 1);
        self.details.get_mut(&itinerary).unwrap()["transporters"][0]["workPhoneNumber"] =
            json!(phone);
    }
    /// The `id` itinerary's detail.
    fn detail(&mut self, id: &str) -> &mut Value {
        &mut self.details.get_mut(id).unwrap()["itineraryDetails"]
    }
    /// The day as a whole read finished `after` milliseconds past the first started: its lists,
    /// and every itinerary its list names.
    fn read(&self, after: i64) -> Capture {
        let mut capture = Capture {
            summaries: self.summaries.clone(),
            route_summaries: self.route_summaries.clone(),
            started_at: self.lists.started_at + after,
            finished_at: self.lists.started_at + after,
            ..self.lists.clone()
        };
        capture.itineraries = routes::listed(&capture.summaries, &capture.scope)
            .into_iter()
            .map(|(id, transporter_id)| routes::ItineraryCapture {
                detail: self.details[&id].to_string(),
                id,
                transporter_id,
            })
            .collect();
        capture
    }
}

/// The tables both route data keep, as Routes declares them.
fn shared_tables() -> Vec<&'static str> {
    let tables = |storage: &str| {
        let mut tables = dispatch_routes::FEATURE.tables.iter();
        tables.find(|(of, _)| *of == storage).unwrap().1
    };
    let live = tables("routelive");
    tables("routedata")
        .iter()
        .copied()
        .filter(|table| live.contains(table))
        .collect()
}
/// A table's rows as its readers read them, without the ids each publication is given: the
/// active publication's, or every row of one the days share; responses inflated.
fn rows(db: &Db, table: &str) -> Vec<Map<String, Value>> {
    let columns = db
        .all(
            &format!("SELECT name FROM pragma_table_info('{table}')"),
            [],
        )
        .unwrap();
    let has = |column: &str| columns.iter().any(|c| s(c, "name") == column);
    let sql = if table == "route_publications" {
        "SELECT * FROM route_publications WHERE active=1 ORDER BY day".to_owned()
    } else if has("publication_id") {
        format!(
            "SELECT t.* FROM {table} t JOIN route_publications p ON p.id=t.publication_id \
             AND p.active=1 ORDER BY 2,3,4"
        )
    } else {
        format!("SELECT * FROM {table} ORDER BY 1,2,3")
    };
    let mut rows = Vec::new();
    for row in db.all(&sql, []).unwrap() {
        let Value::Object(mut row) = row else {
            unreachable!("a row is an object")
        };
        row.remove("publication_id");
        if table == "route_publications" {
            row.remove("id");
        }
        if table == "route_raw" {
            let body: Vec<u8> =
                db.0.query_row(
                    "SELECT r.body FROM route_raw r JOIN route_publications p \
                     ON p.id=r.publication_id AND p.active=1 WHERE r.name=?",
                    [s(&Value::Object(row.clone()), "name")],
                    |r| r.get(0),
                )
                .unwrap();
            row.insert(
                "body".into(),
                json!(String::from_utf8(dispatch_routes::gunzip(&body).unwrap()).unwrap()),
            );
        }
        rows.push(row);
    }
    rows
}
/// How live route data reads otherwise than historical route data, table by table, once the day
/// saved from it is the day's read there: none. Historical route data names the drivers whose
/// itineraries it holds; live route data also those only assigned to a route yet.
fn differences(db: &Store, dsp: &str) -> Vec<String> {
    while db.sweep_routes(dsp).unwrap() {}
    let (live, historical) = (db.routes_live_db(dsp).unwrap(), db.routes_db(dsp).unwrap());
    let mut found = Vec::new();
    for table in shared_tables() {
        let (mut kept, saved) = (rows(&live, table), rows(&historical, table));
        if table == "drivers" {
            let assigned: Vec<Value> = live
                .all(
                    "SELECT DISTINCT a.value id FROM routes r, json_each(r.transporter_ids) a \
                     WHERE a.value NOT IN (SELECT transporter_id FROM itineraries)",
                    [],
                )
                .unwrap();
            let saved_ids: Vec<&Value> = saved.iter().map(|row| &row["transporter_id"]).collect();
            kept.retain(|row| {
                saved_ids.contains(&&row["transporter_id"])
                    || !assigned.iter().any(|a| a["id"] == row["transporter_id"])
            });
        }
        if kept != saved {
            found.push(format!(
                "{table}: {} kept, {} saved",
                kept.len(),
                saved.len()
            ));
        }
    }
    found
}
/// Keeps `read` live, then saves the day live route data holds into historical route data.
async fn keep_and_save(state: &Arc<State>, dsp: &str, read: Capture) -> LiveKept {
    let kept = keep_live(state, dsp, read).await.unwrap();
    assert_eq!(save_live_day(state, dsp).await.unwrap(), Some(true));
    kept
}
/// A hold on a work phone as (driver, phone, since, until).
type Hold = (String, String, i64, Option<i64>);
fn hold(hold: WorkPhoneHold) -> Hold {
    (hold.driver_id, hold.phone, hold.since, hold.until)
}
/// Every hold of the live day, as other features read them.
fn live_holds(db: &Store, dsp: &str) -> Vec<Hold> {
    let live = dispatch_routes_contract::live_for_dsp(db, dsp).unwrap();
    let holds = live
        .expect("Routes is on")
        .work_phone_holds(db, dsp)
        .unwrap();
    holds.into_iter().map(hold).collect()
}
/// Every hold historical route data keeps of `day`, as other features read them.
fn saved_holds(db: &Store, dsp: &str, day: &str) -> Vec<Hold> {
    let routes = dispatch_routes_contract::for_dsp(db, dsp).unwrap();
    let holds = routes.expect("Routes is on").work_phone_holds(db, dsp, day);
    holds.unwrap().into_iter().map(hold).collect()
}

#[tokio::test]
async fn a_live_day_kept_in_place_reads_as_each_read_saved_whole() {
    let (_root, db, dsp, state) = ready();
    let today = today(&db, &dsp);
    let mut day = Day::new(&today.to_string());
    let listed = day.summaries["itinerarySummaries"].clone();
    let started = day.lists.started_at;

    // Routes planned and drivers assigned, nobody dispatched yet: whole, its drivers named.
    day.summaries["itinerarySummaries"] = json!([]);
    let planned = keep_and_save(&state, &dsp, day.read(0)).await;
    assert_eq!((planned.listed, planned.holding), (0, 0));
    assert_eq!(differences(&db, &dsp), Vec::<String>::new(), "planned");
    let live = db.routes_live_db(&dsp).unwrap();
    assert_eq!(live.count("SELECT count(*) FROM drivers", []).unwrap(), 2);

    // Dispatched: each signed in, on a phone of their own.
    day.summaries["itinerarySummaries"] = listed.clone();
    let dispatched = keep_and_save(&state, &dsp, day.read(1)).await;
    assert_eq!(
        (
            dispatched.listed,
            dispatched.holding,
            dispatched.phone_events
        ),
        (2, 2, 2)
    );
    assert_eq!(differences(&db, &dsp), Vec::<String>::new(), "dispatched");

    // One itinerary gone from the list, its driver keeping their phone; a task gone and another
    // changed, an unknown stop gone.
    day.summaries["itinerarySummaries"] = json!([listed[0]]);
    let first = day.detail("itinerary-1");
    first["stops"][1]["tasks"].as_array_mut().unwrap().pop();
    first["stops"][1]["tasks"][0]["taskState"] = json!("UNDELIVERABLE");
    first["unknownStops"] = json!([]);
    let narrowed = keep_and_save(&state, &dsp, day.read(2)).await;
    assert_eq!((narrowed.listed, narrowed.holding), (1, 2));
    assert_eq!(differences(&db, &dsp), Vec::<String>::new(), "narrowed");

    // Both back, the first without its breaks or its first stop; the second route off the list,
    // an address moved, the first driver's record on another phone.
    day.summaries["itinerarySummaries"] = listed.clone();
    day.summaries["itinerarySummaries"][0]["breaks"] = json!([]);
    day.detail("itinerary-1")["stops"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    (day.route_summaries["rmsRouteSummaries"]
        .as_array_mut()
        .unwrap())
    .pop();
    for detail in day.details.values_mut() {
        detail["addresses"][0]["address1"] = json!("9 Moved St");
    }
    day.phone(0, P3);
    keep_and_save(&state, &dsp, day.read(3)).await;
    assert_eq!(differences(&db, &dsp), Vec::<String>::new(), "changed");

    // Read again unchanged, nothing is written, and its switch is confirmed: every hold of the
    // day, the one it ended too, live and as saved, and none of another day.
    let again = keep_and_save(&state, &dsp, day.read(4)).await;
    assert_eq!((again.listed, again.changed), (2, 0));
    assert_eq!(differences(&db, &dsp), Vec::<String>::new(), "unchanged");
    assert_eq!(
        live_holds(&db, &dsp),
        [
            ("driver-1".into(), P1.into(), started + 1, Some(started + 4)),
            ("driver-2".into(), P2.into(), started + 1, None),
            ("driver-1".into(), P3.into(), started + 4, None),
        ]
    );
    assert_eq!(
        saved_holds(&db, &dsp, &today.to_string()),
        live_holds(&db, &dsp)
    );
    let yesterday = (today - chrono::Duration::days(1)).to_string();
    assert_eq!(saved_holds(&db, &dsp, &yesterday), []);
}

#[tokio::test]
async fn a_read_that_isnt_whole_or_isnt_of_today_changes_nothing() {
    let (_root, db, dsp, state) = ready();
    let today = today(&db, &dsp);
    let day = Day::new(&today.to_string());
    // Before the day's first itinerary, a list naming none is whole; one without its list of
    // itineraries is not.
    let mut planned = day.read(0);
    planned.summaries["itinerarySummaries"] = json!([]);
    planned.itineraries.clear();
    keep_live(&state, &dsp, planned.clone()).await.unwrap();
    let mut unlisted = day.read(1);
    unlisted.summaries = json!({"transporters": planned.summaries["transporters"]});
    unlisted.itineraries.clear();
    let refused = keep_live(&state, &dsp, unlisted).await.unwrap_err();
    assert_eq!(refused.code, "routes_capture_invalid");
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    let live = db.routes_live_db(&dsp).unwrap();
    let held = |live: &Db| {
        shared_tables()
            .into_iter()
            .chain(["work_phones_shown"])
            .map(|table| rows(live, table))
            .collect::<Vec<_>>()
    };
    let before = held(&live);

    let mut missing = day.read(2);
    missing.itineraries.pop();
    let mut lost = day.read(2);
    lost.summaries["itinerarySummaries"] = json!([]);
    lost.itineraries.clear();
    let yesterday = Day::new(&(today - chrono::Duration::days(1)).to_string()).read(2);
    let mut finished = day.read(2);
    finished.mode = Mode::Final;
    let mut zoned = day.read(2);
    zoned.scope.timezone = "America/Chicago".into();
    for (read, code, why) in [
        (
            missing,
            "routes_capture_invalid",
            "an itinerary listed missing",
        ),
        (
            lost,
            "routes_capture_invalid",
            "a list naming none once dispatched",
        ),
        (yesterday, "routes_day_not_available", "another day"),
        (finished, "routes_scope_mismatch", "a final read"),
        (
            zoned,
            "routes_scope_mismatch",
            "another scope than the station's",
        ),
    ] {
        let refused = keep_live(&state, &dsp, read).await.unwrap_err();
        assert_eq!(refused.code, code, "{why}");
    }
    // One read under a revision of the Cortex connection it no longer has, as once its
    // credentials were replaced, is no longer the DSP's to keep.
    let revision = db
        .connection_revision(&dsp, cortex::PROVIDER)
        .unwrap()
        .unwrap();
    common::change_connection(&db, &dsp, cortex::PROVIDER).unwrap();
    let stale = dispatch_routes::keep_live(&state, &dsp, day.read(3), revision);
    assert_eq!(stale.await.unwrap_err().code, "connection_changed");
    assert_eq!(held(&live), before);
    // One collected no later than the read kept is no read of today since.
    let earlier = keep_live(&state, &dsp, day.read(0)).await.unwrap();
    assert_eq!((earlier.changed, earlier.phone_events), (0, 0));
    assert_eq!(held(&live), before);

    let owner = common::a_user(&db);
    db.set_feature(&dsp, "routes", false, &owner).unwrap();
    let refused = keep_live(&state, &dsp, day.read(3)).await.unwrap_err();
    assert_eq!(refused.code, "routes_off");
    assert_eq!(
        clear_live(&state, &dsp).await.unwrap_err().code,
        "routes_off"
    );
    assert_eq!(held(&live), before);
}

#[tokio::test]
async fn work_phones_are_settled_across_a_restart_without_a_claim_repeated_or_lost() {
    let (_root, db, dsp, state) = ready();
    let mut day = Day::new(&today(&db, &dsp).to_string());
    let started = day.lists.started_at;
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    day.phone(0, P3);
    keep_live(&state, &dsp, day.read(2)).await.unwrap();
    drop(state);

    // Restarted, the read before isn't kept again, and the switch it showed is confirmed by the
    // next.
    let state = State::new(db.config.clone()).unwrap();
    let repeated = keep_live(&state, &dsp, day.read(2)).await.unwrap();
    assert_eq!((repeated.phone_events, repeated.holding), (0, 2));
    let confirmed = keep_live(&state, &dsp, day.read(3)).await.unwrap();
    assert_eq!(confirmed.phone_events, 1);
    assert_eq!(
        live_holds(&db, &dsp),
        [
            ("driver-1".into(), P1.into(), started + 1, Some(started + 3)),
            ("driver-2".into(), P2.into(), started + 1, None),
            ("driver-1".into(), P3.into(), started + 3, None),
        ]
    );
}

#[tokio::test]
async fn the_live_day_is_saved_once_under_its_reads_key_and_live_route_data_kept() {
    let (_root, db, dsp, state) = ready();
    assert_eq!(save_live_day(&state, &dsp).await.unwrap(), None);
    let today = today(&db, &dsp);
    let day = Day::new(&today.to_string());
    let read = day.read(1);
    let key = format!("live:{today}:{}", read.finished_at);
    keep_live(&state, &dsp, read).await.unwrap();
    // Saved again, as after a restart, it is the day's read still, saved once.
    for _ in 0..2 {
        assert_eq!(save_live_day(&state, &dsp).await.unwrap(), Some(true));
    }
    let historical = db.routes_db(&dsp).unwrap();
    let saved = historical
        .all("SELECT job_id,mode,active FROM route_publications", [])
        .unwrap();
    assert_eq!(saved, [json!({"job_id":key,"mode":"snapshot","active":1})]);
    assert_eq!(saved_holds(&db, &dsp, &today.to_string()).len(), 2);
    let offered = dispatch_routes_contract::live_for_dsp(&db, &dsp)
        .unwrap()
        .unwrap();
    assert_eq!(
        offered.live_day(&db, &dsp).unwrap(),
        Some(today.to_string())
    );

    // A read made once its day had ended, as a window past midnight's, is the day's final.
    let midnight = (today + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis();
    keep_live(&state, &dsp, day.read(midnight - day.lists.started_at + 1))
        .await
        .unwrap();
    assert_eq!(save_live_day(&state, &dsp).await.unwrap(), Some(true));
    let saved = historical
        .one("SELECT mode FROM route_publications WHERE active=1", [])
        .unwrap();
    assert_eq!(saved, Some(json!({"mode":"final"})));
}

#[tokio::test]
async fn clearing_empties_live_route_data_and_gives_its_space_back() {
    let (_root, db, dsp, state) = ready();
    let day = Day::new(&today(&db, &dsp).to_string());
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    let pages = |db: &Store| {
        let live = db.routes_live_db(&dsp).unwrap();
        live.count("SELECT page_count FROM pragma_page_count()", [])
            .unwrap()
    };
    let full = pages(&db);
    clear_live(&state, &dsp).await.unwrap();
    let live = db.routes_live_db(&dsp).unwrap();
    for table in shared_tables().into_iter().chain(["work_phones_shown"]) {
        let held = live
            .count(&format!("SELECT count(*) FROM {table}"), [])
            .unwrap();
        assert_eq!(held, 0, "{table}");
    }
    drop(live);
    assert!(pages(&db) < full, "{} of {full} pages", pages(&db));
    let offered = dispatch_routes_contract::live_for_dsp(&db, &dsp)
        .unwrap()
        .unwrap();
    assert_eq!(offered.live_day(&db, &dsp).unwrap(), None);
    assert_eq!(save_live_day(&state, &dsp).await.unwrap(), None);
    // Off, saving is refused, though live route data holds no day to save.
    let owner = common::a_user(&db);
    db.set_feature(&dsp, "routes", false, &owner).unwrap();
    let refused = save_live_day(&state, &dsp).await.unwrap_err();
    assert_eq!(refused.code, "routes_off");
}

#[tokio::test]
async fn a_saved_day_is_cleared_and_a_cleared_one_gives_its_space_back() {
    let (_root, db, dsp, state) = ready();
    let day = Day::new(&today(&db, &dsp).to_string());
    let free = |db: &Store| {
        let live = db.routes_live_db(&dsp).unwrap();
        live.count("SELECT freelist_count FROM pragma_freelist_count()", [])
            .unwrap()
    };
    let saved = |db: &Store| {
        let historical = db.routes_db(&dsp).unwrap();
        historical
            .all("SELECT job_id FROM route_publications WHERE active=1", [])
            .unwrap()
    };
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    assert_eq!(save_and_clear_live(&state, &dsp).await.unwrap(), Some(true));
    let offered = dispatch_routes_contract::live_for_dsp(&db, &dsp)
        .unwrap()
        .unwrap();
    assert_eq!(offered.live_day(&db, &dsp).unwrap(), None);
    let history = saved(&db);
    assert_eq!(history.len(), 1);
    assert_eq!(free(&db), 0);

    // Emptied by an earlier call that couldn't give the space back, as when its VACUUM failed:
    // the next gives it back, saving nothing.
    keep_live(&state, &dsp, day.read(2)).await.unwrap();
    let live = db.routes_live_db(&dsp).unwrap();
    live.transaction(|| {
        for table in shared_tables().into_iter().chain(["work_phones_shown"]) {
            live.exec(&format!("DELETE FROM {table}"), [])?;
        }
        Ok(())
    })
    .unwrap();
    drop(live);
    assert!(free(&db) > 0);
    assert_eq!(save_and_clear_live(&state, &dsp).await.unwrap(), None);
    assert_eq!(free(&db), 0);
    assert_eq!(saved(&db), history);
}

#[tokio::test]
async fn driver_match_hears_of_each_driver_live_route_data_names_first() {
    let (_root, db, dsp, state) = ready();
    let mut day = Day::new(&today(&db, &dsp).to_string());
    let listed = day.summaries["itinerarySummaries"].clone();
    day.summaries["itinerarySummaries"] = json!([listed[0]]);
    day.route_summaries["transporters"] = json!([day.route_summaries["transporters"][0]]);
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    assert_eq!(changed(&dsp), 1);
    keep_live(&state, &dsp, day.read(2)).await.unwrap();
    assert_eq!(changed(&dsp), 1);
    day.summaries["itinerarySummaries"] = listed;
    keep_live(&state, &dsp, day.read(3)).await.unwrap();
    assert_eq!(changed(&dsp), 2);
}

#[tokio::test]
async fn a_number_once_held_as_a_work_phone_is_one_live_or_saved() {
    let (_root, db, dsp, state) = ready();
    let mut day = Day::new(&today(&db, &dsp).to_string());
    let routes = dispatch_routes_contract::for_dsp(&db, &dsp)
        .unwrap()
        .unwrap();
    let was = |phone: &str| routes.was_work_phone(&db, &dsp, phone).unwrap();
    assert!(!was(P1));

    // The first driver claims their phone; the second, not signed in yet, only shows theirs.
    day.summaries["itinerarySummaries"][1]["lastDriverEventTime"] = json!(0);
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    assert!(was(P1) && was("+1 (555) 010-0001"), "held live");
    assert!(!was(P2), "shown, never held");
    assert!(!was("0100001"), "fewer than ten digits");

    // Saved, then gone from live route data: held on a saved day.
    save_and_clear_live(&state, &dsp).await.unwrap();
    assert_eq!(live_holds(&db, &dsp), []);
    assert!(was(P1), "held on a saved day");
}

#[tokio::test]
async fn features_hear_who_holds_a_work_phone_changed_once_its_kept() {
    let (_root, db, dsp, state) = ready();
    let mut day = Day::new(&today(&db, &dsp).to_string());
    // Each driver claims their phone.
    keep_live(&state, &dsp, day.read(1)).await.unwrap();
    assert_eq!(phones_heard(&dsp), [(2, 0)]);
    // A read moving no phone, its switch shown once, isn't heard; the next confirms it.
    day.phone(0, P3);
    keep_live(&state, &dsp, day.read(2)).await.unwrap();
    assert_eq!(phones_heard(&dsp), [(2, 0)]);
    keep_live(&state, &dsp, day.read(3)).await.unwrap();
    assert_eq!(phones_heard(&dsp), [(2, 0), (3, 0)]);
    // Saved with its holds, then emptied at the stop.
    save_and_clear_live(&state, &dsp).await.unwrap();
    assert_eq!(phones_heard(&dsp), [(2, 0), (3, 0), (3, 3), (0, 3)]);
}
