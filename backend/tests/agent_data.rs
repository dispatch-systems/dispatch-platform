//! The agent API's answers over a DSP's real shapes of data: one driver joined across
//! Paycom, routes, meal breaks and DVIC by Driver Match, everyone's numbers for a day,
//! and refusals that say what to fix instead of guessing.
mod common;
use dispatch_backend::{
    State,
    agents::{Caller, data},
    collectors::Provider,
    contracts::AgentKeyRequest,
    db::{Store, s},
    meals::{self, Scope},
    routedata::{self, Mode, Request},
    workforce,
};
use serde_json::{Value, json};
use std::sync::Arc;

const DAY: &str = "2026-09-12";

fn day() -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(DAY, "%Y-%m-%d").unwrap()
}

/// A DSP whose sources all name one driver: Paycom's E002, Amazon's driver-1.
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    db.enable_all_features(&id).unwrap();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    db.collector(&id, Provider::Cortex)
        .unwrap()
        .exec("UPDATE connections SET enabled=1,status='ready'", [])
        .unwrap();
    // Paycom's roster, with E002 named as Amazon writes the fixture's first driver.
    let mut roster = workforce::fixture_date("UTC", Some(day())).unwrap();
    for employee in roster["employees"].as_array_mut().unwrap() {
        if employee["code"] == "E002" {
            employee["name"] = json!("DRIVER, FIXTURE");
        }
    }
    db.publish(&id, &roster).unwrap();
    // The day's routes.
    let request = Request::parse(&json!({"collection":"routes","mode":"final","date":DAY,
        "station":"TST1","timezone":"UTC","dspName":"Test Owner","dspAbbreviation":"NLOG"}))
    .unwrap()
    .unwrap();
    let capture = routedata::fixture(&request).unwrap();
    let jobs = db
        .enqueue_routes(&id, None, "agent-routes", Some(DAY), Mode::Final, 1)
        .unwrap();
    let job = s(&jobs[0], "id").to_owned();
    let staged = db.stage_routes(&id, &job, capture).unwrap();
    db.publish_routes(&id, &job, &staged).unwrap();
    db.jobs
        .exec("UPDATE jobs SET status='succeeded' WHERE id=?", [&job])
        .unwrap();
    // The day's meal breaks, from the same transporter ID.
    let scope = Scope {
        date: DAY.into(),
        station: "TST1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "UTC".into(),
    };
    let mut meals = meals::fixture(&scope);
    meals.itineraries.truncate(1);
    meals.itineraries[0].transporter_id = "driver-1".into();
    meals.itineraries[0].driver = "Fixture Driver".into();
    db.publish_meals(&id, "agent-meals", &meals, &scope)
        .unwrap();
    // Two inspections, one of them short.
    let dvic = db.dvic(&id).unwrap();
    dvic.exec(
        "INSERT INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,report_date,\
         modified_at,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at) VALUES \
         ('report-1','company-1','NLOG','TST1','key-1','DVIC','2026-W37',?1,0,'sha','rev-1',2,1,?1,?1,?1)",
        [DAY],
    )
    .unwrap();
    dvic.exec(
        "INSERT INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) \
         VALUES ('rev-1','report-1','sha',0,?,'[]')",
        [DAY],
    )
    .unwrap();
    for (key, seconds, short) in [("inspection-1", 412.0, 0), ("inspection-2", 41.0, 1)] {
        dvic.exec(
            "INSERT INTO dvic_inspections(company_id,inspection_key,dsp_code,station,start_date,\
             transporter_id,transporter_name,vin,fleet_type,inspection_type,inspection_status,\
             start_time,end_time,duration_seconds,minimum_seconds,short,report_date,\
             source_modified_at,revision_id) VALUES ('company-1',?,'NLOG','TST1',?,'driver-1',\
             'Fixture Driver','VIN1','CDV','PRE_TRIP','COMPLETE','07:01','07:08',?,90,?,?,0,'rev-1')",
            rusqlite::params![key, DAY, seconds, short, DAY],
        )
        .unwrap();
    }
    drop(dvic);
    db.match_drivers(&id).unwrap();
    (root, db, id)
}

fn owner(db: &Store) -> String {
    let user = db
        .platform
        .one("SELECT id FROM users WHERE email='owner@example.test'", [])
        .unwrap()
        .unwrap();
    s(&user, "id").to_owned()
}

/// A key's caller, as the access check would sign it in.
fn caller(db: &Store, dsps: &[&str], locations: bool) -> Caller {
    let request = AgentKeyRequest::parse(&json!({
        "name": format!("key {}", dsps.len() * 2 + usize::from(locations)),
        "allDsps": dsps.is_empty(), "dsps": dsps, "access": "read", "tools": "full",
        "locations": locations, "expiresAt": null,
    }))
    .unwrap();
    let made = db.create_agent_key(&owner(db), &request).unwrap();
    db.authenticate_agent(&made.token, "test").unwrap()
}

/// Answers inside a read, as a route would.
async fn ask(
    state: &Arc<State>,
    question: impl FnOnce(&Store, &State) -> data::Answer + Send + 'static,
) -> (u16, Value) {
    let shared = state.clone();
    state
        .read(move |db| data::settle(question(db, &shared)))
        .await
        .unwrap()
}

#[tokio::test]
async fn one_driver_is_one_person_across_every_source() {
    let (_root, db, id) = ready();
    let me = caller(&db, &[&id], false);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();

    let who = me.clone();
    let (status, report) = ask(&state, move |db, state| {
        data::driver(db, state, &who, "Fixture Driver", &json!({"date": DAY}))
    })
    .await;
    assert_eq!(status, 200, "{report}");
    assert_eq!(report["driver"]["paycomIds"], json!(["E002"]));
    assert_eq!(report["driver"]["amazonIds"], json!(["driver-1"]));
    assert_eq!(report["period"]["from"], DAY);
    assert_eq!(report["period"]["days"], 1);
    let day = &report["days"][0];
    assert_eq!(day["date"], DAY);
    assert_eq!(day["routes"][0]["transporterId"], "driver-1");
    assert!(day["timecard"]["hours"].as_f64().unwrap() > 0.0);
    assert!(day["mealBreaks"]["status"].is_string());
    assert_eq!(day["inspections"].as_array().unwrap().len(), 2);
    assert_eq!(report["totals"]["routes"], 1);
    assert_eq!(report["totals"]["shortInspections"], 1);
    // Each source says which days it holds; missing days never read as zero.
    assert_eq!(report["coverage"]["routes"]["days"], json!([DAY]));
    assert_eq!(report["coverage"]["timecards"]["days"], json!([DAY]));

    // Everyone's numbers for the day, the joined driver in one row.
    let who = me.clone();
    let (status, team) = ask(&state, move |db, state| {
        data::team(
            db,
            state,
            &who,
            &json!({"date": DAY, "metrics": "stops_completed,packages_delivered,hours_worked,inspections"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{team}");
    let rows = team["rows"].as_array().unwrap();
    let joined = rows
        .iter()
        .find(|r| r["driver"]["name"] == "Fixture Driver")
        .unwrap();
    assert!(joined["stops_completed"].is_number());
    assert!(joined["hours_worked"].as_f64().unwrap() > 0.0);
    assert_eq!(joined["inspections"], 2);
    // Someone Paycom lists but who drove no route has hours and no stops, not zero stops.
    let office = rows
        .iter()
        .find(|r| r["driver"]["name"] != "Fixture Driver" && r["hours_worked"].is_number())
        .unwrap();
    assert!(office["stops_completed"].is_null());
    // Highest first by the first metric, those without it last.
    let stops: Vec<Option<f64>> = rows.iter().map(|r| r["stops_completed"].as_f64()).collect();
    let mut sorted = stops.clone();
    sorted.sort_by(|a, b| match (a, b) {
        (Some(x), Some(y)) => y.total_cmp(x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    assert_eq!(stops, sorted);

    // The day's routes, and one itinerary without addresses for a key not allowed them.
    let who = me.clone();
    let (_, routes) = ask(&state, move |db, state| {
        data::routes(db, state, &who, &json!({"date": DAY}))
    })
    .await;
    assert_eq!(routes["final"], true);
    assert_eq!(routes["totals"]["routes"], 2);
    let itinerary = routes["itineraries"][0]["itinerary"]
        .as_str()
        .unwrap()
        .to_owned();
    let who = me.clone();
    let wanted = itinerary.clone();
    let (_, stops) = ask(&state, move |db, state| {
        data::route(db, state, &who, &wanted, &json!({"date": DAY}))
    })
    .await;
    assert_eq!(stops["addresses"], false);
    assert!(
        stops["stops"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["place"].is_null())
    );
    // A package is found however its tracking ID is written.
    let tracking = stops["stops"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| s["packages"].as_array().unwrap())
        .find_map(|p| p["tracking"].as_str())
        .unwrap()
        .to_lowercase();
    let who = me.clone();
    let (status, package) = ask(&state, move |db, state| {
        data::package(db, state, &who, &tracking, &json!({}))
    })
    .await;
    assert_eq!(status, 200, "{package}");
    assert_eq!(package["events"][0]["itinerary"], itinerary.as_str());
    assert!(package["events"][0]["place"].is_null());

    let who = me.clone();
    let (_, meals) = ask(&state, move |db, state| {
        data::meal_breaks(db, state, &who, &json!({"date": DAY}))
    })
    .await;
    assert_eq!(meals["collected"], true);
    let who = me.clone();
    let (_, dvic) = ask(&state, move |db, state| {
        data::dvic(db, state, &who, &json!({"date": DAY, "short": "true"}))
    })
    .await;
    assert_eq!(dvic["inspections"].as_array().unwrap().len(), 1);
    assert_eq!(dvic["inspections"][0]["driver"]["name"], "Fixture Driver");
    let who = me;
    let (_, status_answer) = ask(&state, move |db, _| data::status(db, &who, &json!({}))).await;
    assert_eq!(status_answer["sources"]["routes"]["latestDay"], DAY);
    assert_eq!(status_answer["sources"]["timecards"]["enabled"], true);
}

#[tokio::test]
async fn unclear_requests_are_refused_with_what_to_fix() {
    let (_root, db, id) = ready();
    db.new_dsp("Cedar Ridge Delivery", "UTC", &owner(&db), false)
        .unwrap();
    let everywhere = caller(&db, &[], false);
    let one = caller(&db, &[&id], true);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let refused = |answer: (u16, Value)| {
        (
            answer.0,
            answer.1["error"].as_str().unwrap_or("").to_owned(),
        )
    };

    let who = everywhere.clone();
    let (status, body) = ask(&state, move |db, state| {
        data::drivers(db, state, &who, &json!({}))
    })
    .await;
    assert_eq!(
        (status, body["error"].clone()),
        (400, json!("dsp_required"))
    );
    assert!(body["choices"].as_array().unwrap().len() >= 2);

    for (query, code) in [
        (json!({"metrics": "steps"}), "unknown_metric"),
        (json!({"metrics": "clock_in"}), "day_metric"),
        (json!({"week": "39"}), "unknown_parameter"),
        (json!({"period": "someday"}), "invalid_period"),
        (
            json!({"from": "2026-01-01", "to": "2026-09-01"}),
            "period_too_long",
        ),
        (json!({"sort": "mood"}), "invalid_sort"),
        (
            json!({"metrics": "hours_worked", "sort": "stops_completed"}),
            "invalid_sort",
        ),
    ] {
        let who = one.clone();
        let answer = ask(&state, move |db, state| data::team(db, state, &who, &query)).await;
        assert_eq!(refused(answer), (400, code.to_owned()), "{code}");
    }
    let who = one.clone();
    let answer = ask(&state, move |db, state| {
        data::driver(db, state, &who, "Driver", &json!({}))
    })
    .await;
    assert_eq!(refused(answer), (400, "driver_ambiguous".to_owned()));
    let who = one.clone();
    let answer = ask(&state, move |db, state| {
        data::routes(db, state, &who, &json!({"period": "last week"}))
    })
    .await;
    assert_eq!(refused(answer), (400, "unknown_parameter".to_owned()));
    // Everyone's timecards come a day at a time; a period needs a driver.
    let who = one.clone();
    let answer = ask(&state, move |db, state| {
        data::timecards(db, state, &who, &json!({"period": "last week"}))
    })
    .await;
    assert_eq!(refused(answer), (400, "one_day_only".to_owned()));

    // A key allowed addresses sees where each stop was.
    let who = one;
    let (_, routes) = ask(&state, move |db, state| {
        data::routes(db, state, &who.clone(), &json!({"date": DAY})).and_then(|routes| {
            let itinerary = routes["itineraries"][0]["itinerary"]
                .as_str()
                .unwrap()
                .to_owned();
            data::route(db, state, &who, &itinerary, &json!({"date": DAY}))
        })
    })
    .await;
    assert_eq!(routes["addresses"], true);
    assert!(
        routes["stops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["place"].is_object())
    );
}
