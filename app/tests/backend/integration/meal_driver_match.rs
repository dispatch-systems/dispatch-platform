//! Timecard's meal breaks join drivers to employees through Driver Match alone.
use dispatch_core::testing as common;
use dispatch_core::{
    accounts::{Auth, Context},
    db::{Store, s},
    manifest::people::DriverSource,
};
use dispatch_cortex::{self as cortex, discovery::Scope, meals};
use dispatch_driver_match::DriverMatchStore;
use dispatch_paycom::fixtures;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// A day in the demo period of Sep 6 to 19, 2026, within a week of its start.
const DEMO_DAY: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
fn actor(db: &Store) -> String {
    common::a_user(db)
}
fn seed(db: &Store, id: &str) -> (String, String) {
    // A fixed day inside the demo period, so the period's first day has timecards whatever
    // today is: the fixture seeds the seven days up to "today", and the comparison reads
    // the period's first day.
    let mut workforce = fixtures::fixture_date("America/Los_Angeles", Some(DEMO_DAY)).unwrap();
    let date = s(&workforce, "from").to_owned();
    // Provider formatting differences should join without a saved identity.
    workforce["employees"][0]["name"] = json!("Driver, Demo");
    let cards = workforce["timecards"].as_array_mut().unwrap();
    for card in cards {
        if card["employeeCode"] == "E002" {
            card["punches"] = json!([]);
        }
        if card["employeeCode"] == "E003" {
            card["punches"] =
                json!([{"in":null,"out":"12:00","hours":null,"inKind":null,"outKind":"OUT LUNCH"}]);
            card["status"] = json!("Missing punch");
        }
    }
    db.publish_timecards(id, &workforce).unwrap();
    let scope = Scope {
        date: date.clone(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "America/Los_Angeles".into(),
    };
    let mut capture = meals::fixture(&scope);
    capture.itineraries[0].driver = "Demo Driver".into();
    let transporter = capture.itineraries[0].transporter_id.clone();
    db.publish_meals(id, "job-meals", &capture, &scope).unwrap();
    (date, transporter)
}
/// A platform owner's view of the DSP, to decide in Driver Match.
fn context(db: &Store, id: &str) -> Context {
    db.enable_all_features(id).unwrap();
    let auth = Auth {
        user: serde_json::from_value(
            json!({"id":actor(db),"email":"","firstName":"","lastName":"","platformOwner":true}),
        )
        .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: dispatch_core::foundation::config::Site::Admin,
        scope: None,
    };
    db.context(&auth, id, "driver_match.manage").unwrap()
}
fn person(db: &Store, id: &str, source: DriverSource, external: &str) -> String {
    db.driver_match(id)
        .unwrap()
        .drivers
        .into_iter()
        .find(|d| d.ids.iter().any(|i| i.source == source && i.id == external))
        .unwrap()
        .code
}
fn amazon() -> DriverSource {
    DriverSource::new(&cortex::PEOPLE)
}
fn paycom() -> DriverSource {
    DriverSource::new(&dispatch_paycom::PEOPLE)
}
fn rows(data: &Value) -> &Vec<Value> {
    data["rows"].as_array().unwrap()
}
#[test]
fn drivers_join_employees_through_driver_match() {
    dispatch_backend::install();
    let (_root, db, id) = common::bootstrapped();
    let (date, transporter) = seed(&db, &id);
    let compare = |db: &Store| {
        db.meal_comparison(&id, &date, "UTC")
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()
    };
    // Until Driver Match has given them a person, as while a collection runs, the driver
    // has a row of their own.
    let data = compare(&db);
    assert_eq!(data["timezone"], "America/Los_Angeles");
    assert_eq!(data["drivers"][0]["matchType"], "unmatched");
    assert!(data["drivers"][0]["paycomCode"].is_null());
    assert_eq!(rows(&data).len(), 12);
    // Driver Match joins them to their employee by name, and its decisions move them.
    let c = context(&db, &id);
    db.match_drivers(&id).unwrap();
    let data = compare(&db);
    assert_eq!(data["drivers"][0]["matchType"], "name");
    assert_eq!(data["drivers"][0]["paycomCode"], "E001");
    assert_eq!(rows(&data).len(), 11);
    assert!(
        !rows(&data)
            .iter()
            .any(|r| r["paycom"]["employeeCode"] == "E002")
    );
    assert!(
        rows(&data)
            .iter()
            .any(|r| r["paycom"]["employeeCode"] == "E003")
    );
    let demo = person(&db, &id, amazon(), &transporter);
    db.split_driver(&c, &demo, amazon(), &transporter).unwrap();
    let data = compare(&db);
    assert_eq!(data["drivers"][0]["matchType"], "unmatched");
    assert_eq!(rows(&data).len(), 12);
    let alone = person(&db, &id, amazon(), &transporter);
    let second = person(&db, &id, paycom(), "E002");
    db.merge_drivers(&c, &alone, &second).unwrap();
    let data = compare(&db);
    assert_eq!(data["drivers"][0]["matchType"], "saved");
    assert_eq!(data["drivers"][0]["paycomCode"], "E002");
    let linked = rows(&data)
        .iter()
        .find(|r| r["id"] == "paycom:E002")
        .unwrap();
    assert!(linked["paycom"].is_null());
    assert_eq!(linked["cortex"].as_array().unwrap().len(), 1);
    assert!(linked["cortex"][0]["lastDelivery"].is_string());
    // The previous release reads the same link where the meal-break page kept it.
    let legacy = db
        .dsp(&id)
        .unwrap()
        .setting("employees.provider_links", Value::Null)
        .unwrap();
    assert_eq!(legacy["links"][0]["cortexId"], json!(transporter));
    assert_eq!(legacy["links"][0]["paycomCode"], "E002");
    assert_eq!(
        db.meal_comparison(&id, "2020-01-01", "UTC")
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["rows"],
        json!([])
    );
    assert!(
        db.meal_comparison(&id, "not-a-date", "UTC")
            .map(|value| serde_json::to_value(value).unwrap())
            .is_err()
    );
    assert_eq!(
        db.collector(&id, cortex::PROVIDER)
            .unwrap()
            .one("SELECT count(*) n FROM meal_delivery_events", [])
            .unwrap()
            .unwrap()["n"],
        0
    );
}
