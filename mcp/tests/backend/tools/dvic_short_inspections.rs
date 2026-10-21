//! DVIC short inspections, called as an agent calls it, against reports DVIC published.
use super::*;
use crate::{
    api::types::ToolLevel,
    testing::{self, agent, call_tool, serving},
    toolbox::Failure,
};
use dispatch_core::testing::{self as common, bootstrapped, platform_owner};
use dispatch_cortex::{
    self as cortex,
    discovery::{CollectionRequest, Scope},
    dvic::{Capture, Request},
};
use dispatch_dvic::DvicStore;
use serde_json::{Value, json};

fn day(value: &str) -> NaiveDate {
    parse(value).unwrap()
}
fn search(arguments: Value) -> Search {
    serde_json::from_value(arguments).unwrap()
}
fn ranged(arguments: Value, today: &str) -> Result<Option<(String, String)>, String> {
    range(&search(arguments), day(today))
        .map(|range| range.map(|(from, to)| (from.to_string(), to.to_string())))
        .map_err(|refusal| refusal.message)
}
fn days(from: &str, to: &str) -> Result<Option<(String, String)>, String> {
    Ok(Some((from.into(), to.into())))
}

#[test]
fn a_search_names_its_days_one_way_in_the_dsps_own_time() {
    // A Wednesday.
    let today = "2026-10-07";
    assert_eq!(ranged(json!({}), today), Ok(None));
    assert_eq!(
        ranged(json!({"date": "2026-10-01"}), today),
        days("2026-10-01", "2026-10-01")
    );
    assert_eq!(
        ranged(json!({"from": "2026-09-28", "to": "2026-10-02"}), today),
        days("2026-09-28", "2026-10-02")
    );
    assert_eq!(
        ranged(json!({"from": "2026-10-01"}), today),
        days("2026-10-01", today)
    );
    assert_eq!(
        ranged(json!({"last_days": 3}), today),
        days("2026-10-05", today)
    );
    assert_eq!(ranged(json!({"last_days": 1}), today), days(today, today));
    // Weeks run Sunday to Saturday, as the DVIC page shows them.
    assert_eq!(
        ranged(json!({"week": "this"}), today),
        days("2026-10-04", today)
    );
    assert_eq!(
        ranged(json!({"week": "last"}), today),
        days("2026-09-27", "2026-10-03")
    );
    assert_eq!(
        ranged(json!({"week": "this"}), "2026-10-04"),
        days("2026-10-04", "2026-10-04")
    );

    for (wrong, message) in [
        (json!({"date": "2026-10-01", "week": "last"}), "Give one of"),
        (json!({"to": "2026-10-01"}), "`to` ends a range"),
        (
            json!({"from": "2026-10-03", "to": "2026-10-01"}),
            "comes after",
        ),
        (json!({"date": "2026-10-08"}), "after today"),
        (json!({"date": "Oct 1"}), "`date` is a day"),
        (json!({"last_days": 0}), "1 to 366"),
        (json!({"last_days": 367}), "1 to 366"),
        (json!({"from": "2025-01-01"}), "a year at most"),
    ] {
        let refused = ranged(wrong.clone(), today).unwrap_err();
        assert!(refused.contains(message), "{wrong}: {refused}");
    }
}

#[test]
fn a_driver_goes_by_every_name_id_and_contact_the_directory_has_for_them() {
    use dispatch_core::manifest::people::{Person, PersonRole};
    let driver = |id: &str, name: &str| dvic::Driver {
        id: id.into(),
        name: name.into(),
        first_date: "2026-09-19".into(),
    };
    let known = [
        driver("A1", "Cam Hayes"),
        driver("A2", "Cam Hayes"),
        driver("B1", "Avery Morgan"),
        driver("C1", "Avery Mendez"),
    ];
    let person = |code: &str, names: &[&str], ids: &[&str], email: &str| Person {
        code: code.into(),
        name: names[0].into(),
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        role: PersonRole::Driver,
        title: None,
        ids: BTreeMap::from([(
            dispatch_cortex::SOURCE,
            ids.iter().map(|id| (*id).to_owned()).collect(),
        )]),
        emails: vec![email.into()],
        phones: vec![],
        confirmed: false,
        photo: None,
    };
    // Cam's two transporter IDs are one person, whom another system writes in full.
    let people = Everyone::new(
        vec![
            person(
                "CAMHAY",
                &["Cameron Hayes", "Cam Hayes"],
                &["A1", "A2"],
                "cam@example.com",
            ),
            person("AVEMOR", &["Avery Morgan"], &["B1"], "avery@example.com"),
        ],
        BTreeMap::new(),
    );
    let ids = |asked: &str| {
        matched(
            dispatch_cortex::SOURCE,
            &known,
            &people,
            &[asked.to_owned()],
        )
        .map(|(found, _)| found.iter().map(|driver| driver.id.clone()).collect())
        .map_err(|refusal| refusal.code)
    };
    assert_eq!(ids("Cameron"), Ok(vec!["A1".to_owned(), "A2".to_owned()]));
    assert_eq!(
        ids("CAM@example.com"),
        Ok(vec!["A1".to_owned(), "A2".to_owned()])
    );
    // Whom the directory has yet to tell apart goes by DVIC's own spelling.
    assert_eq!(ids("Mendez"), Ok(vec!["C1".to_owned()]));
    assert_eq!(ids("Avery"), Err("driver_ambiguous".to_owned()));
}

/// DVIC, at a DSP whose station and Cortex connection are set, with DVIC switched on.
pub(crate) fn ready() -> (tempfile::TempDir, Store, String) {
    testing::install();
    let (root, db, id) = bootstrapped();
    common::set_dsp(&db, &id, "Fixture Delivery", "America/Los_Angeles").unwrap();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &id, cortex::PROVIDER).unwrap();
    db.set_feature(&id, "dvic", true, &platform_owner(&db))
        .unwrap();
    (root, db, id)
}
/// The request a collection of `weeks` carries, dated today as publication binds it.
pub(crate) fn request(weeks: &[&str]) -> Request {
    let date = chrono::Utc::now()
        .with_timezone(&chrono_tz::America::Los_Angeles)
        .date_naive()
        .to_string();
    Request::parse(&json!({"collection":"dvic","station":"TST1","weeks":weeks,
        "date":date,"timezone":"America/Los_Angeles",
        "dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}))
    .unwrap()
    .unwrap()
}
/// Publishes `capture` as DVIC's collection of its weeks does.
pub(crate) fn publish(db: &Store, id: &str, key: &str, capture: &Capture) {
    let job = db
        .enqueue_dvic(
            id,
            None,
            key,
            Some(&capture.weeks[0]),
            capture.weeks.len(),
            None,
        )
        .unwrap();
    let job = dispatch_core::db::s(&job, "id").to_owned();
    let queued: Value = serde_json::from_str(&db.job_row(&job, Some(id)).unwrap().request).unwrap();
    let weeks: Vec<&str> = queued["weeks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|week| week.as_str().unwrap())
        .collect();
    let scope: Scope = match request(&weeks).scope_request() {
        CollectionRequest::Discover(discovery) => {
            discovery.scope("area-fixture", "company-fixture").unwrap()
        }
        CollectionRequest::Scoped(scope) => scope,
    };
    db.publish_dvic(id, &job, capture, &scope).unwrap();
    common::set_job_status(db, &job, "succeeded").unwrap();
}
/// Two weeks of reports: Fixture Driver short on Sep 19, then as Fixture Driver Jr on Sep 26
/// beside a step van's Avery Morgan and Avery Mendez.
fn published(db: &Store, id: &str) {
    publish(
        db,
        id,
        "older",
        &cortex::dvic::fixture(&request(&["2026-W38"])).unwrap(),
    );
    let mut newer = cortex::dvic::fixture(&request(&["2026-W39"])).unwrap();
    let rows = newer.reports[0].rows.as_mut().unwrap();
    rows[0].transporter_name = "Fixture Driver Jr".into();
    for (n, (name, fleet, start, end, seconds)) in [
        ("Avery Morgan", "SV", "10:00:00", "10:03:20", 200.0),
        ("Avery Mendez", "CDV", "11:00:00", "11:01:00", 60.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut row = rows[0].clone();
        row.transporter_id = format!("driver-{}", n + 2);
        row.transporter_name = name.into();
        row.vin = format!("1FIXTURE00000000{}", n + 2);
        row.fleet_type = fleet.into();
        row.start_time = format!("2026-09-26 {start}");
        row.end_time = format!("2026-09-26 {end}");
        row.duration = seconds;
        rows.push(row);
    }
    publish(db, id, "newer", &newer);
}
fn found(answer: Answer<crate::toolbox::Answered>) -> (Value, String) {
    let answered = answer.unwrap();
    (answered.data, answered.text.unwrap_or_default())
}
fn refused(answer: Answer<crate::toolbox::Answered>) -> (String, Vec<String>) {
    match answer {
        Err(Failure::Refused(refusal)) => (refusal.code, refusal.choices),
        other => panic!("not refused: {other:?}"),
    }
}
fn names_of(found: &Value, list: &str, field: &str) -> Vec<String> {
    found[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item[field].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn finds_everyone_short_on_the_days_asked_with_how_far_dvic_is_collected() {
    let (_root, db, id) = ready();
    published(&db, &id);
    let caller = agent(&db, &[&id], &[("dvic_short_inspections", ToolLevel::Read)]);
    let state = serving(db);
    let call = |arguments: Value| {
        let (state, caller) = (state.clone(), caller.clone());
        async move { call_tool(&state, &caller, "dvic_short_inspections", arguments).await }
    };

    let (week, text) = found(call(json!({"from": "2026-09-13", "to": "2026-09-26"})).await);
    assert_eq!(week["dsp"], "Fixture Delivery");
    assert_eq!(
        (&week["from"], &week["to"]),
        (&json!("2026-09-13"), &json!("2026-09-26"))
    );
    // Each daily report covers the eight days before it: here those of Sep 20 and 27.
    assert_eq!(
        week["collected"],
        json!({"from": "2026-09-12", "through": "2026-09-26"})
    );
    assert_eq!(week["complete"], true);
    assert_eq!(
        names_of(&week, "drivers", "name"),
        ["Fixture Driver Jr", "Avery Mendez", "Avery Morgan"]
    );
    assert_eq!(week["drivers"][0]["short_inspections"], 2);
    assert_eq!(
        week["drivers"][0]["days"],
        json!(["2026-09-19", "2026-09-26"])
    );
    assert_eq!(week["drivers"][2]["most_short_by_seconds"], 100.0);
    // Newest first, each with its minimum.
    assert_eq!(week["inspections"][0]["driver"], "Avery Mendez");
    assert_eq!(week["inspections"][0]["vehicle_type"], "CDV");
    assert_eq!(week["inspections"][3]["date"], "2026-09-19");
    assert_eq!(week["more"], 0);
    assert_eq!(
        text,
        "4 short DVIC inspections by 3 drivers at Fixture Delivery, Sep 13 to Sep 26."
    );

    let (one_day, _) = found(call(json!({"date": "2026-09-19"})).await);
    assert_eq!(names_of(&one_day, "inspections", "driver_id"), ["driver-1"]);

    // A driver by name, part of one, or Amazon ID; with no days, all DVIC has for them.
    let (avery, text) = found(call(json!({"drivers": ["avery morgan"]})).await);
    assert_eq!(names_of(&avery, "inspections", "driver"), ["Avery Morgan"]);
    assert_eq!(avery["from"], "2026-09-26");
    // Up to today, beyond what is collected, so the answer says so.
    assert_eq!(avery["complete"], false);
    assert!(
        text.ends_with(
            "DVIC is collected from Sep 12 through Sep 26; days outside that are not known."
        ),
        "{text}"
    );
    let (by_id, _) = found(
        call(json!({"drivers": ["driver-1"], "from": "2026-09-01", "to": "2026-09-30"})).await,
    );
    assert_eq!(by_id["drivers"][0]["short_inspections"], 2);
    let (mendez, _) =
        found(call(json!({"drivers": ["Mendez", "Nobody Here"], "date": "2026-09-26"})).await);
    assert_eq!(names_of(&mendez, "drivers", "name"), ["Avery Mendez"]);
    assert_eq!(mendez["not_found"], json!(["Nobody Here"]));

    // A name that could be two drivers asks which.
    let (code, choices) = refused(call(json!({"drivers": ["Avery"]})).await);
    assert_eq!(code, "driver_ambiguous");
    assert_eq!(
        choices,
        ["Avery Mendez (driver-3)", "Avery Morgan (driver-2)"]
    );
    let (code, _) = refused(call(json!({"date": "2026-09-19", "last_days": 3})).await);
    assert_eq!(code, "invalid_input");
}

#[tokio::test]
async fn says_when_nothing_is_collected_and_is_refused_where_dvic_is_off() {
    let (_root, db, id) = ready();
    let caller = agent(&db, &[&id], &[("dvic_short_inspections", ToolLevel::Read)]);
    let owner = platform_owner(&db);
    let state = serving(db);
    let (nothing, text) = found(
        call_tool(
            &state,
            &caller,
            "dvic_short_inspections",
            json!({"last_days": 3}),
        )
        .await,
    );
    assert_eq!(nothing["collected"], Value::Null);
    assert_eq!(nothing["complete"], false);
    assert!(nothing["inspections"].as_array().unwrap().is_empty());
    assert!(
        text.ends_with("DVIC has collected nothing at this DSP yet, so nothing is known."),
        "{text}"
    );

    let off = id.clone();
    state
        .run(move |db| db.set_feature(&off, "dvic", false, &owner).map(|_| ()))
        .await
        .unwrap();
    let (code, _) = refused(call_tool(&state, &caller, "dvic_short_inspections", json!({})).await);
    assert_eq!(code, "switched_off");
}
