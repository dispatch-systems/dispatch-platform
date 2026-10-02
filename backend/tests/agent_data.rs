//! The agent API's answers over a DSP's real shapes of data: one driver joined across
//! Paycom, routes, meal breaks and DVIC by Driver Match, everyone's numbers for a day,
//! and refusals that say what to fix instead of guessing.
mod common;
use dispatch_backend::{
    State,
    agents::{Caller, data, synthetic},
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
        .one("SELECT id FROM users WHERE platform_owner=1 LIMIT 1", [])
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

/// A table's rows, and where a column sits in them.
fn rows(table: &Value) -> &Vec<Value> {
    table["rows"].as_array().unwrap()
}
fn col(table: &Value, name: &str) -> usize {
    table["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|c| c == name)
        .unwrap_or_else(|| panic!("no column {name} in {table}"))
}
fn row<'a>(table: &'a Value, column: &str, value: &str) -> &'a Value {
    let at = col(table, column);
    rows(table)
        .iter()
        .find(|r| r[at] == value)
        .unwrap_or_else(|| panic!("no row with {column}={value} in {table}"))
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
    assert_eq!(report["understood"]["driver"]["paycom"], json!(["E002"]));
    assert_eq!(
        report["understood"]["driver"]["amazon"],
        json!(["driver-1"])
    );
    assert_eq!(report["understood"]["from"], DAY);
    // One line for the day, from every source.
    let days = &report["days"];
    let line = row(days, "date", DAY);
    assert_eq!(line[col(days, "route")], "CX101");
    assert!(line[col(days, "hours")].as_f64().unwrap() > 0.0);
    assert_eq!(line[col(days, "inspections")], 2);
    assert_eq!(line[col(days, "short")], 1);
    assert_eq!(report["totals"]["routes"], 1);
    assert_eq!(report["totals"]["short_inspections"], 1);
    // Each source says how many of the days it holds; missing days are never zero.
    assert_eq!(
        report["coverage"]["routes"],
        json!({"collected": 1, "of": 1})
    );

    // Everyone's numbers, the joined driver in one row, the team's totals beside them.
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
    let table = &team["rows"];
    let joined = row(table, "driver", "Fixture Driver");
    assert!(joined[col(table, "stops_completed")].is_number());
    assert!(joined[col(table, "hours_worked")].as_f64().unwrap() > 0.0);
    assert_eq!(joined[col(table, "inspections")], 2);
    assert_eq!(team["totals"]["inspections"], 2);
    // Someone Paycom lists who drove no route has hours and no stops, not zero stops.
    let (stops, worked) = (col(table, "stops_completed"), col(table, "hours_worked"));
    assert!(
        rows(table)
            .iter()
            .any(|r| r[worked].is_number() && r[stops].is_null())
    );
    // Highest first by the first metric, those without it last.
    let order: Vec<Option<f64>> = rows(table).iter().map(|r| r[stops].as_f64()).collect();
    let first_none = order
        .iter()
        .position(Option::is_none)
        .unwrap_or(order.len());
    assert!(order[first_none..].iter().all(Option::is_none));

    // Lowest first when asked, and the answer says which way it sorted.
    let who = me.clone();
    let (_, fewest) = ask(&state, move |db, state| {
        data::team(
            db,
            state,
            &who,
            &json!({"date": DAY, "metrics": "packages_delivered", "order": "lowest"}),
        )
    })
    .await;
    assert_eq!(fewest["sorted"], "by packages_delivered, lowest first");
    let delivered: Vec<i64> = rows(&fewest["rows"])
        .iter()
        .filter_map(|r| r[1].as_i64())
        .collect();
    assert!(delivered.windows(2).all(|w| w[0] <= w[1]), "{fewest}");

    // The day's routes, then one route's problems, with no addresses for this key.
    let who = me.clone();
    let (_, routes) = ask(&state, move |db, state| {
        data::routes(db, state, &who, &json!({"date": DAY}))
    })
    .await;
    assert_eq!(routes["final"], true);
    assert_eq!(routes["totals"]["routes"], 2);
    let who = me.clone();
    let (status, second) = ask(&state, move |db, state| {
        data::route(db, state, &who, "cx102", &json!({"date": DAY}))
    })
    .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["driver"], "Second Driver");
    assert_eq!(second["outcomes"], json!({"delivered": 1, "returned": 1}));
    let problems = &second["not_delivered"];
    assert_eq!(rows(problems).len(), 1);
    assert!(
        !problems["columns"]
            .as_array()
            .unwrap()
            .contains(&json!("address"))
    );

    // Packages: a count, grouped as asked, listed only when asked.
    let who = me.clone();
    let (status, counted) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"date": DAY, "group_by": "driver,outcome"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{counted}");
    assert_eq!(counted["packages"], 4);
    assert!(counted.get("list").is_none());
    let groups = &counted["groups"];
    let returned = rows(groups)
        .iter()
        .find(|r| r[col(groups, "outcome")] == "returned")
        .unwrap();
    assert_eq!(returned[col(groups, "driver")], "Second Driver");
    assert_eq!(returned[col(groups, "packages")], 1);
    let who = me.clone();
    let (_, mine) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"date": DAY, "driver": "Fixture Driver", "outcome": "delivered", "list": "true"}),
        )
    })
    .await;
    assert_eq!(mine["packages"], 2, "{mine}");
    // Nothing with both an outcome and a reason: the answer says what that reason came with.
    let who = me.clone();
    let (_, none) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"date": DAY, "outcome": "returned", "reason": "doorstep"}),
        )
    })
    .await;
    assert_eq!(none["packages"], 0);
    assert_eq!(
        none["note"],
        "None were returned. With that reason there were: 3 delivered."
    );
    assert_eq!(rows(&mine["list"]).len(), 2);
    // A package is found however its tracking ID is written.
    let who = me.clone();
    let (status, package) = ask(&state, move |db, state| {
        data::package(db, state, &who, "tba000000000002", &json!({}))
    })
    .await;
    assert_eq!(status, 200, "{package}");
    // Its pickup at the station and its drop-off, on each route that carried it.
    assert_eq!(rows(&package["events"]).len(), 4);

    // Meal breaks only for drivers Cortex had a route for; nobody else is listed.
    let who = me.clone();
    let (_, meals) = ask(&state, move |db, state| {
        data::meal_breaks(db, state, &who, &json!({"date": DAY}))
    })
    .await;
    assert_eq!(meals["collected"], true);
    let drivers = &meals["drivers"];
    assert_eq!(rows(drivers).len(), 1);
    assert_eq!(rows(drivers)[0][col(drivers, "driver")], "Fixture Driver");
    assert!(meals.get("withoutRoute").is_none());

    // DVIC per driver, the short ones counted.
    let who = me.clone();
    let (_, dvic) = ask(&state, move |db, state| {
        data::dvic(db, state, &who, &json!({"date": DAY, "short": "true"}))
    })
    .await;
    assert_eq!(
        (dvic["inspections"].clone(), dvic["short"].clone()),
        (json!(2), json!(1))
    );
    assert_eq!(
        rows(&dvic["drivers"]),
        &vec![json!(["Fixture Driver", 2, 1, 41])]
    );

    let who = me.clone();
    let (_, status_answer) = ask(&state, move |db, _| data::status(db, &who, &json!({}))).await;
    assert_eq!(status_answer["sources"]["routes"]["latestDay"], DAY);
    // A day nothing was collected for has no totals at all, never zeros.
    let who = me;
    let (_, nothing) = ask(&state, move |db, state| {
        data::routes(db, state, &who, &json!({"date": "2026-09-13"}))
    })
    .await;
    assert_eq!(nothing["totals"], Value::Null);
    assert!(
        nothing["note"]
            .as_str()
            .unwrap()
            .contains("unknown, not zero")
    );
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
            json!({"from": "2025-01-01", "to": "2026-09-01"}),
            "period_too_long",
        ),
        (json!({"sort": "mood"}), "invalid_sort"),
        (
            json!({"metrics": "hours_worked", "sort": "stops_completed"}),
            "invalid_sort",
        ),
        (json!({"cursor": "next"}), "invalid_cursor"),
        // Hours are read a day at a time, so a year of them is refused.
        (
            json!({"metrics": "hours_worked", "period": "last 200 days"}),
            "period_too_long",
        ),
    ] {
        let who = one.clone();
        let answer = ask(&state, move |db, state| data::team(db, state, &who, &query)).await;
        assert_eq!(refused(answer), (400, code.to_owned()), "{code}");
    }
    for (query, status, code) in [
        (json!({"reason": "lunch"}), 400, "unknown_reason"),
        (json!({"group_by": "colour"}), 400, "invalid_group_by"),
        (
            json!({"group_by": "driver,day,route"}),
            400,
            "invalid_group_by",
        ),
        (json!({"outcome": "lost"}), 400, "invalid_parameter"),
    ] {
        let who = one.clone();
        let answer = ask(&state, move |db, state| {
            data::packages(db, state, &who, &query)
        })
        .await;
        assert_eq!(refused(answer), (status, code.to_owned()), "{code}");
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
    let who = one.clone();
    let answer = ask(&state, move |db, state| {
        data::route(db, state, &who, "CX999", &json!({"date": DAY}))
    })
    .await;
    assert_eq!(refused(answer.clone()), (404, "route_not_found".to_owned()));
    assert_eq!(answer.1["choices"], json!(["CX101", "CX102"]));

    // A key allowed addresses sees where each package went, and may group by address.
    let who = one.clone();
    let (_, all) = ask(&state, move |db, state| {
        data::route(
            db,
            state,
            &who,
            "CX101",
            &json!({"date": DAY, "detail": "full"}),
        )
    })
    .await;
    let list = &all["packages_list"];
    let address = col(list, "address");
    assert!(rows(list).iter().any(|r| r[address].is_string()));
    let who = one;
    let (status, places) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"date": DAY, "group_by": "address"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{places}");
    let who = everywhere;
    let answer = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"dsp": id, "date": DAY, "group_by": "address"}),
        )
    })
    .await;
    assert_eq!(refused(answer).1, "locations_off");
}

/// A question asked of the agent API, as a test asks it.
type Question = Box<dyn FnOnce(&Store, &State, &Caller) -> data::Answer + Send>;

#[tokio::test]
async fn package_group_and_list_cursors_advance_independently_within_the_final_budget() {
    let (_root, db) = common::seeded();
    let world = synthetic::seed(&db).unwrap();
    let me = caller(&db, &[s(&world, "dsp")], false);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let who = me.clone();
    let (status, first) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"group_by":"day,route","list":"true","limit":"1"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{first}");
    assert!(first.to_string().len() <= data::BUDGET);
    let groups_cursor = first["groups"]["page"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let list_cursor = first["list"]["page"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let who = me.clone();
    let (status, second) = ask(&state, move |db, state| data::packages(db, state, &who,
        &json!({"group_by":"day,route","list":"true","limit":"1", "groups_cursor":groups_cursor}))).await;
    assert_eq!(status, 200, "{second}");
    assert!(second.to_string().len() <= data::BUDGET);
    assert_eq!(second["list"]["rows"], first["list"]["rows"]);
    assert_ne!(second["groups"]["rows"], first["groups"]["rows"]);
    assert!(second["groups"]["page"].is_null(), "{second}");
    let who = me.clone();
    let (status, third) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &who,
            &json!({"group_by":"day,route","list":"true","limit":"1", "cursor":list_cursor}),
        )
    })
    .await;
    assert_eq!(status, 200, "{third}");
    assert_eq!(third["groups"], first["groups"]);
    assert_ne!(third["list"]["rows"], first["list"]["rows"]);
    let (status, invalid) = ask(&state, move |db, state| {
        data::packages(
            db,
            state,
            &me,
            &json!({"group_by":"day,route", "groups_cursor":"100"}),
        )
    })
    .await;
    assert_eq!(status, 400);
    assert_eq!(invalid["error"], "invalid_parameter");
}

/// Every tool's answer to a plain question stays small at a real DSP's size: two weeks of
/// a dozen drivers' routes, timecards, meal breaks and inspections.
#[tokio::test]
async fn answers_stay_within_their_budgets() {
    let (_root, db) = common::seeded();
    let world = synthetic::seed(&db).unwrap();
    let dsp = s(&world, "dsp").to_owned();
    let me = caller(&db, &[&dsp], false);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let last = s(&world, "to").to_owned();
    // (question, the tool's answer, the most bytes it may take)
    let cases: Vec<(&str, Question, usize)> = vec![
        (
            "packages delivered by a driver last 30 days",
            Box::new(|db, state, who: &Caller| {
                data::packages(
                    db,
                    state,
                    who,
                    &json!({"driver": "Taylor Brooks", "outcome": "delivered"}),
                )
            }),
            1_000,
        ),
        (
            "returns by reason last night",
            Box::new(|db, state, who: &Caller| {
                data::packages(
                    db,
                    state,
                    who,
                    &json!({"date": "last night", "outcome": "returned", "group_by": "driver,reason"}),
                )
            }),
            2_000,
        ),
        (
            "short DVIC drivers",
            Box::new(|db, state, who: &Caller| {
                data::dvic(db, state, who, &json!({"short": "true"}))
            }),
            2_000,
        ),
        (
            "a driver's month",
            Box::new(|db, state, who: &Caller| {
                data::driver(db, state, who, "Taylor Brooks", &json!({}))
            }),
            5_000,
        ),
        (
            "the team's month",
            Box::new(|db, state, who: &Caller| data::team(db, state, who, &json!({}))),
            3_000,
        ),
        (
            "a day's routes",
            Box::new(|db, state, who: &Caller| data::routes(db, state, who, &json!({}))),
            3_000,
        ),
        (
            "everyone's timecards for a day",
            Box::new(|db, state, who: &Caller| data::timecards(db, state, who, &json!({}))),
            3_000,
        ),
        (
            "a day's meal breaks",
            Box::new(|db, state, who: &Caller| data::meal_breaks(db, state, who, &json!({}))),
            3_000,
        ),
        (
            "everyone per day for two weeks",
            Box::new(|db, state, who: &Caller| {
                data::team(
                    db,
                    state,
                    who,
                    &json!({"period": "last 14 days", "per": "day", "metrics": "packages_delivered"}),
                )
            }),
            data::BUDGET,
        ),
        (
            "every package of a month",
            Box::new(|db, state, who: &Caller| {
                data::packages(db, state, who, &json!({"list": "true", "limit": "500"}))
            }),
            data::BUDGET,
        ),
    ];
    for (question, tool, budget) in cases {
        let who = me.clone();
        let (status, answer) = ask(&state, move |db, state| tool(db, state, &who)).await;
        assert_eq!(status, 200, "{question}: {answer}");
        let size = answer.to_string().len();
        eprintln!("{question}: {size} bytes");
        assert!(
            size <= budget,
            "{question}: {size} bytes, budget {budget}\n{answer}"
        );
    }
    // A route's problems come back small; every package of it only when asked, in pages.
    let who = me.clone();
    let day = last.clone();
    let (_, routes) = ask(&state, move |db, state| {
        data::routes(db, state, &who, &json!({"date": day}))
    })
    .await;
    let code = rows(&routes["routes"])[0][0].as_str().unwrap().to_owned();
    let (who, day, wanted) = (me.clone(), last.clone(), code.clone());
    let (_, problems) = ask(&state, move |db, state| {
        data::route(db, state, &who, &wanted, &json!({"date": day}))
    })
    .await;
    assert!(problems.to_string().len() <= 2_000, "{problems}");
    let (who, day) = (me, last);
    let (_, everything) = ask(&state, move |db, state| {
        data::route(
            db,
            state,
            &who,
            &code,
            &json!({"date": day, "detail": "full", "limit": "500"}),
        )
    })
    .await;
    assert!(everything.to_string().len() <= data::BUDGET);
}

#[tokio::test]
async fn meal_duration_answers_preserve_unknown_totals_and_overnight_clocks() {
    let (_root, db, id) = ready();
    let me = caller(&db, &[&id], false);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let cases = [
        (
            "open",
            json!([{"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"}]),
            None,
        ),
        (
            "mixed",
            json!([
                {"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"},
                {"in":"12:30","inKind":"IN LUNCH","out":"14:00","outKind":"OUT LUNCH"}
            ]),
            None,
        ),
        (
            "complete",
            json!([{"in":"08:00","out":"12:00"},{"in":"12:30","out":"17:00"}]),
            Some(30),
        ),
        (
            "overnight",
            json!([
                {"in":"22:00","inKind":"IN DAY","out":"23:50","outKind":"OUT LUNCH"},
                {"in":"00:20","inKind":"IN LUNCH","out":"06:00","outKind":"OUT DAY"}
            ]),
            Some(30),
        ),
    ];
    for (case, punches, minutes) in cases {
        let tenant = id.clone();
        state.run(move |db| {
            db.collector(&tenant, Provider::Paycom)?.exec(
                "UPDATE timecards SET status='Complete',punches=? WHERE employee_code='E002' AND date=?",
                [punches.to_string(), DAY.into()],
            )?;
            let cortex = db.collector(&tenant, Provider::Cortex)?;
            cortex.exec("DELETE FROM meal_records", [])?;
            let start = if case == "overnight" { "2026-09-12T23:50:00Z" } else { "2026-09-12T12:00:00Z" };
            let end = match case {
                "open" => None,
                "overnight" => Some("2026-09-13T00:20:00Z"),
                _ => Some("2026-09-12T12:30:00Z"),
            };
            cortex.exec(
                "INSERT INTO meal_records SELECT id,'fixture-itinerary','first',NULL,?,?,NULL,'unavailable',? \
                 FROM meal_publications WHERE active=1",
                rusqlite::params![start,end,if end.is_some() { "unavailable" } else { "pending" }],
            )?;
            if case == "mixed" {
                cortex.exec(
                    "INSERT INTO meal_records SELECT id,'fixture-itinerary','second',NULL,\
                     '2026-09-12T14:00:00Z',NULL,NULL,'unavailable','pending' FROM meal_publications WHERE active=1",
                    [],
                )?;
            }
            Ok(())
        }).await.unwrap();
        let who = me.clone();
        let (status, cards) = ask(&state, move |db, state| {
            data::timecards(
                db,
                state,
                &who,
                &json!({"date":DAY,"driver":"Fixture Driver"}),
            )
        })
        .await;
        assert_eq!(status, 200, "{case}: {cards}");
        let table = &cards["timecards"];
        let card = row(table, "date", DAY);
        assert_eq!(
            card[col(table, "lunch_minutes")],
            json!(minutes),
            "{case}: {cards}"
        );
        if case == "overnight" {
            assert_eq!(card[col(table, "out")], "06:00 +1 day");
        }
        let who = me.clone();
        let (status, meals) = ask(&state, move |db, state| {
            data::meal_breaks(db, state, &who, &json!({"date":DAY}))
        })
        .await;
        assert_eq!(status, 200, "{case}: {meals}");
        let table = &meals["drivers"];
        let meal = row(table, "driver", "Fixture Driver");
        assert_eq!(
            meal[col(table, "minutes")],
            json!(minutes),
            "{case}: {meals}"
        );
        if case == "overnight" {
            assert_eq!(meal[col(table, "meal")], "23:50–00:20");
        }
        let who = me.clone();
        let (status, totals) = ask(&state, move |db, state| {
            data::team(
                db,
                state,
                &who,
                &json!({"date":DAY,"metrics":"lunch_minutes"}),
            )
        })
        .await;
        assert_eq!(status, 200, "{case}: {totals}");
        let table = &totals["rows"];
        assert_eq!(
            row(table, "driver", "Fixture Driver")[col(table, "lunch_minutes")],
            json!(minutes)
        );
        if minutes.is_none() {
            assert!(
                totals["totals"]["lunch_minutes"].is_null(),
                "a partial sum must not become the team's total: {totals}"
            );
        }
    }
}

#[tokio::test]
async fn driver_periods_keep_historical_sync_and_meal_context_across_batches() {
    let (_root, db, id) = ready();
    let mut history = workforce::fixture_date("UTC", Some("2026-09-05".parse().unwrap())).unwrap();
    history["employees"][1]["name"] = json!("DRIVER, FIXTURE");
    history["collectedAt"] = json!("2099-01-01T00:00:00Z");
    db.publish(&id, &history).unwrap();
    let employee = history["employees"][1].clone();
    let sync = json!({
        "employees":[employee], "from":"2026-09-06", "to":"2026-09-19",
        "collectedAt":"2099-01-02T00:00:00Z",
        "timecards": (6..=19).map(|day| json!({
            "employeeCode":"E002","date":format!("2026-09-{day:02}"),"hours":6.5,
            "status":"Complete","punches":[
                {"in":"08:00","out":"12:00","hours":4},
                {"in":"12:30","out":"15:00","hours":2.5}
            ],
        })).collect::<Vec<_>>()
    });
    db.collector(&id, Provider::Paycom)
        .unwrap()
        .exec(
            "INSERT INTO employee_timecard_syncs VALUES ('E002','2026-09-06','2026-09-19',?,?)",
            ["2099-01-02T00:00:00Z", sync.to_string().as_str()],
        )
        .unwrap();
    for date in ["2026-09-01", "2026-09-08", DAY] {
        let scope = Scope {
            date: date.into(),
            station: "TST1".into(),
            service_area_id: "area-demo".into(),
            provider: "provider-demo".into(),
            timezone: "UTC".into(),
        };
        let mut capture = meals::fixture(&scope);
        capture.itineraries[0].transporter_id = "driver-1".into();
        capture.itineraries[0].driver = "Fixture Driver".into();
        db.publish_meals(&id, &format!("period-{date}"), &capture, &scope)
            .unwrap();
    }
    let me = caller(&db, &[&id], false);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let who = me.clone();
    let (status, report) = ask(&state, move |db, state| {
        data::driver(
            db,
            state,
            &who,
            "Fixture Driver",
            &json!({"from":"2026-09-01","to":DAY}),
        )
    })
    .await;
    assert_eq!(status, 200, "{report}");
    let table = &report["days"];
    assert_eq!(rows(table).len(), 12, "{report}");
    let worked = col(table, "hours");
    assert_eq!(row(table, "date", "2026-09-05")[worked], 8.0);
    assert_eq!(row(table, "date", "2026-09-06")[worked], 6.5);
    assert_eq!(row(table, "date", "2026-09-08")[worked], 6.5);
    assert_eq!(row(table, "date", DAY)[worked], 6.5);
    assert_eq!(report["coverage"]["timecards"]["collected"], 12);
    assert_eq!(report["coverage"]["mealBreaks"]["collected"], 3);
    let who = me.clone();
    let (status, cards) = ask(&state, move |db, state| {
        data::timecards(
            db,
            state,
            &who,
            &json!({"from":"2026-09-01","to":DAY,"driver":"Fixture Driver"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{cards}");
    assert_eq!(rows(&cards["timecards"]).len(), 12);
}

/// The scorecard questions a DSP asks, answered small from the weekly scorecard: repeated
/// feedback at an address, contact compliance, Netradyne events and the week's tiers.
#[tokio::test]
async fn scorecard_questions_come_back_small() {
    let (_root, db) = common::seeded();
    let world = synthetic::seed(&db).unwrap();
    assert!(world["scorecard_weeks"].as_u64().unwrap() >= 1, "{world}");
    let dsp = s(&world, "dsp").to_owned();
    let me = caller(&db, &[&dsp], true);
    let no_places = caller(&db, &[&dsp], false);
    let actor = owner(&db);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let asked = |answer: &Value| {
        let size = answer.to_string().len();
        eprintln!("{size} bytes: {answer}");
        size
    };

    // Houses that complained more than once, from feedback joined to the routes' addresses.
    let who = me.clone();
    let (status, houses) = ask(&state, move |db, state| {
        data::feedback(
            db,
            state,
            &who,
            &json!({"group_by": "address", "min_count": "2"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{houses}");
    assert!(asked(&houses) <= 2_000);
    let groups = &houses["groups"];
    // The three houses the synthetic DSP plants complaints at lead; others may repeat too.
    let top: Vec<&str> = rows(groups)
        .iter()
        .take(3)
        .map(|r| r[0].as_str().unwrap().split(',').next().unwrap())
        .collect();
    for house in [
        "2007 Synthetic Ave",
        "9003 Synthetic Ave",
        "5012 Synthetic Ave",
    ] {
        assert!(top.contains(&house), "{house} in {houses}");
    }
    assert!(rows(groups).iter().all(|r| r[1].as_i64().unwrap() >= 2));
    assert_eq!(houses["unplaced"], 0);

    // Contact compliance: the drivers whose returns say they did not call or text.
    let who = me.clone();
    let (status, missed) = ask(&state, move |db, state| {
        data::returns(
            db,
            state,
            &who,
            &json!({"contact": "missed", "group_by": "driver"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{missed}");
    assert!(asked(&missed) <= 2_000);
    assert_eq!(missed["returns"], missed["contact_missed"]);
    let total: i64 = rows(&missed["groups"])
        .iter()
        .map(|r| r[1].as_i64().unwrap())
        .sum();
    assert_eq!(missed["returns"].as_i64().unwrap(), total);

    // One driver's Netradyne events come back one by one.
    let who = me.clone();
    let (status, safety) = ask(&state, move |db, state| {
        data::safety(
            db,
            state,
            &who,
            &json!({"driver": "Taylor Brooks", "period": "last 14 days"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{safety}");
    assert!(asked(&safety) <= 2_000);
    assert_eq!(
        safety["events"].as_u64().unwrap() as usize,
        rows(&safety["list"]).len()
    );

    // The latest week's scorecard, lowest scores first.
    let who = me.clone();
    let (status, week) = ask(&state, move |db, state| {
        data::weekly(db, state, &who, &json!({}))
    })
    .await;
    assert_eq!(status, 200, "{week}");
    assert!(asked(&week) <= 3_000);
    assert_eq!(week["posted"], true);
    assert_eq!(week["dsp"]["contact_compliance"], "Silver");
    let scores: Vec<f64> = rows(&week["drivers"])
        .iter()
        .map(|r| r[1].as_f64().unwrap())
        .collect();
    assert_eq!(scores.len(), 11);
    assert!(scores.windows(2).all(|w| w[0] <= w[1]));

    // Praise is counted only when asked for, as by naming a kind of it.
    let who = me.clone();
    let (status, praise) = ask(&state, move |db, state| {
        data::feedback(db, state, &who, &json!({"type": "delivered_with_care"}))
    })
    .await;
    assert_eq!(status, 200, "{praise}");
    assert_eq!(praise["understood"]["feedback"], "positive");
    assert!(praise["feedback"].as_u64().unwrap() > 0, "{praise}");

    // A list beside groups is its first page only: the cursor pages the groups.
    let who = me.clone();
    let (status, both) = ask(&state, move |db, state| {
        data::returns(
            db,
            state,
            &who,
            &json!({"group_by": "driver", "list": "true", "limit": "10"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{both}");
    assert!(both["returns"].as_u64().unwrap() > 10, "{both}");
    assert_eq!(rows(&both["list"]).len(), 10);
    assert!(both["list"]["page"].get("next_cursor").is_none(), "{both}");

    // Approved disputes stay events but leave every count of what counts.
    let who = me.clone();
    let (status, events) = ask(&state, move |db, state| {
        data::safety(db, state, &who, &json!({"group_by": "driver"}))
    })
    .await;
    assert_eq!(status, 200, "{events}");
    let sum = |table: &Value, column: &str| -> i64 {
        let at = col(table, column);
        rows(table).iter().map(|r| r[at].as_i64().unwrap()).sum()
    };
    assert_eq!(
        sum(&events["by_type"], "counting"),
        events["counting"].as_i64().unwrap()
    );
    assert_eq!(
        sum(&events["groups"], "counting"),
        events["counting"].as_i64().unwrap()
    );
    assert!(
        events["counting"].as_i64() < events["events"].as_i64(),
        "{events}"
    );

    // A type Amazon never recorded is refused; one it did, matched in part, is answered.
    let who = me.clone();
    let (status, body) = ask(&state, move |db, state| {
        data::safety(db, state, &who, &json!({"type": "juggling"}))
    })
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("unknown_type"))
    );
    let who = me.clone();
    let (status, body) = ask(&state, move |db, state| {
        data::safety(
            db,
            state,
            &who,
            &json!({"type": "speeding", "period": "last week"}),
        )
    })
    .await;
    assert_eq!(status, 200, "{body}");

    // This week's scorecard is not posted yet: refused as unknown, never answered as zero.
    let who = me.clone();
    let (status, recent) = ask(&state, move |db, state| {
        data::returns(db, state, &who, &json!({"period": "today"}))
    })
    .await;
    assert_eq!(
        (status, recent["error"].as_str()),
        (404, Some("not_posted_yet"))
    );
    let said = recent["message"].as_str().unwrap();
    assert!(
        said.contains("unknown, not zero") && said.contains("packages"),
        "{said}"
    );

    // The status names the latest week collected.
    let who = me.clone();
    let (_, status) = ask(&state, move |db, _| data::status(db, &who, &json!({}))).await;
    assert_eq!(status["sources"]["scorecard"]["enabled"], true);
    assert_eq!(
        status["sources"]["scorecard"]["latestWeek"],
        week["understood"]["week"]
    );

    // Feedback by address needs a key allowed addresses.
    let (status, body) = ask(&state, move |db, state| {
        data::feedback(db, state, &no_places, &json!({"group_by": "address"}))
    })
    .await;
    assert_eq!(
        (status, body["error"].as_str()),
        (403, Some("locations_off"))
    );

    // Addresses come from the routes: with routes off there are none to give.
    let off = dsp.clone();
    let switcher = actor.clone();
    state
        .run(move |db| db.set_feature(&off, "routes", false, &switcher).map(|_| ()))
        .await
        .unwrap();
    let who = me.clone();
    let (status, body) = ask(&state, move |db, state| {
        data::feedback(db, state, &who, &json!({"group_by": "address"}))
    })
    .await;
    assert_eq!((status, body["error"].as_str()), (403, Some("source_off")));
    let who = me.clone();
    let (status, body) = ask(&state, move |db, state| {
        data::feedback(db, state, &who, &json!({"list": "true"}))
    })
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(
        !body["list"]["columns"]
            .as_array()
            .unwrap()
            .contains(&json!("address"))
    );

    // The scorecard is its own feature: the Timecard switched off leaves it answering, and
    // the Scorecard switched off refuses every scorecard tool by name.
    let off = dsp.clone();
    let switcher = actor.clone();
    state
        .run(move |db| {
            db.set_feature(&off, "timecard", false, &switcher)
                .map(|_| ())
        })
        .await
        .unwrap();
    let scorecard_tools = ["feedback", "safety", "returns", "scorecard"];
    for id in scorecard_tools {
        let who = me.clone();
        let (status, body) = ask(&state, move |db, state| {
            data::ask(data::catalog::endpoint(id), db, state, &who, "", &json!({}))
        })
        .await;
        assert_eq!(status, 200, "{id}: {body}");
    }
    let off = dsp.clone();
    state
        .run(move |db| db.set_feature(&off, "scorecard", false, &actor).map(|_| ()))
        .await
        .unwrap();
    for id in scorecard_tools {
        let who = me.clone();
        let (status, body) = ask(&state, move |db, state| {
            data::ask(data::catalog::endpoint(id), db, state, &who, "", &json!({}))
        })
        .await;
        assert_eq!(
            (status, body["error"].as_str()),
            (403, Some("source_off")),
            "{id}"
        );
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .starts_with("Northline Logistics has Scorecard switched off"),
            "{body}"
        );
    }
    let (_, status) = ask(&state, move |db, _| data::status(db, &me, &json!({}))).await;
    assert_eq!(status["sources"]["scorecard"], json!({"enabled": false}));
}

/// Every tool tells an agent when what it reads is switched off for the DSP: refused by the
/// switch's name, or answered with that source's figures null and named. A new tool must
/// decide which, or this fails.
#[tokio::test]
async fn every_tool_says_when_its_feature_is_switched_off() {
    let (_root, db) = common::seeded();
    let world = synthetic::seed(&db).unwrap();
    let dsp = s(&world, "dsp").to_owned();
    let me = caller(&db, &[&dsp], true);
    let actor = owner(&db);
    // Every page an agent reads; their tabs and Driver Match go with them.
    for feature in ["timecard", "routes", "dvic", "scorecard"] {
        db.set_feature(&dsp, feature, false, &actor).unwrap();
    }
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    for endpoint in data::catalog::ENDPOINTS {
        let who = me.clone();
        let named = match endpoint.id {
            "driver" => "Taylor Brooks",
            "route" => "SYN-1",
            "package" => "TBA0000000000",
            _ => "",
        };
        let (status, body) = ask(&state, move |db, state| {
            data::ask(endpoint, db, state, &who, named, &json!({}))
        })
        .await;
        let id = endpoint.id;
        match (endpoint.source, id) {
            (Some(source), _) => {
                assert_eq!(
                    (status, body["error"].as_str()),
                    (403, Some("source_off")),
                    "{id}: {body}"
                );
                let said = format!("Northline Logistics has {} switched off", source.switch());
                assert!(
                    body["message"].as_str().unwrap().starts_with(&said),
                    "{id}: {body}"
                );
            }
            (None, "whoami" | "metrics" | "drivers") => assert_eq!(status, 200, "{id}: {body}"),
            (None, "status") => {
                for key in ["timecards", "mealBreaks", "routes", "dvic", "scorecard"] {
                    assert_eq!(body["sources"][key], json!({"enabled": false}), "{key}");
                }
            }
            (None, "driver") => {
                assert_eq!(status, 200, "{body}");
                for (key, value) in body["totals"].as_object().unwrap() {
                    assert!(value.is_null(), "{key} is unknown, not {value}");
                }
                assert_eq!(
                    body["switched_off"],
                    json!(["Routes", "Timecard", "Timecard · Meal Breaks", "DVIC"])
                );
            }
            (None, "team") => assert_eq!(
                (status, body["error"].as_str()),
                (403, Some("source_off")),
                "{body}"
            ),
            (None, other) => panic!("decide how `{other}` answers with its source switched off"),
        }
    }
}
