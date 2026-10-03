#[path = "../../../../../core/db/tests/support/common.rs"]
mod common;
use dispatch_backend::{
    accounts::{Auth, Context},
    collectors::{
        cortex::{self, discovery::Scope, meals},
        paycom::{self, fixtures},
    },
    contracts::DriverSource,
    db::{Store, s},
    driver_match::DriverMatchStore,
    workforce::TimecardStore,
};
use serde_json::{Value, json};

/// Timecard, the Driver Match it joins drivers through, and both collectors it keeps.
fn install() {
    common::install(
        &[
            &dispatch_backend::collectors::paycom::COLLECTOR,
            &dispatch_backend::collectors::cortex::COLLECTOR,
        ],
        &[
            &dispatch_backend::feature_manifests::driver_match::FEATURE,
            &dispatch_backend::feature_manifests::timecard::FEATURE,
        ],
    );
}

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
fn rows(data: &Value) -> &Vec<Value> {
    data["rows"].as_array().unwrap()
}
#[test]
fn drivers_join_employees_through_driver_match_and_names_cover_the_rest() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let (date, transporter) = seed(&db, &id);
    let compare = |db: &Store| {
        db.meal_comparison(&id, &date, "UTC")
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()
    };
    // Before Driver Match has given anyone a person, a unique name joins the driver.
    let data = compare(&db);
    assert_eq!(data["timezone"], "America/Los_Angeles");
    assert_eq!(rows(&data).len(), 11);
    assert_eq!(data["drivers"][0]["matchType"], "name");
    assert_eq!(data["drivers"][0]["paycomCode"], "E001");
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
    // Driver Match gives the same answer, and its decisions move the driver.
    let c = context(&db, &id);
    db.match_drivers(&id).unwrap();
    assert_eq!(compare(&db)["drivers"][0]["paycomCode"], "E001");
    let demo = person(&db, &id, DriverSource::Amazon, &transporter);
    db.split_driver(&c, &demo, DriverSource::Amazon, &transporter)
        .unwrap();
    let data = compare(&db);
    assert_eq!(data["drivers"][0]["matchType"], "unmatched");
    assert_eq!(rows(&data).len(), 12);
    let alone = person(&db, &id, DriverSource::Amazon, &transporter);
    let second = person(&db, &id, DriverSource::Paycom, "E002");
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
/// Adds a meal-break driver to the day's publication, as a collection would have.
fn add_driver(db: &Store, id: &str, transporter: &str, name: &str) {
    let cortex = db.collector(id, cortex::PROVIDER).unwrap();
    let itinerary = format!("itinerary-{transporter}");
    cortex
        .exec(
            "INSERT INTO meal_itineraries SELECT publication_id,?1,?2,?3,route_code,observed_at,\
             route_complete,delivery_coverage,meal_state FROM meal_itineraries LIMIT 1",
            [itinerary.as_str(), transporter, name],
        )
        .unwrap();
    cortex
        .exec(
            "INSERT INTO meal_records SELECT publication_id,?1,meal_id,last_delivery_at,\
             started_at,ended_at,first_delivery_at,before_status,after_status FROM meal_records \
             LIMIT 1",
            [itinerary.as_str()],
        )
        .unwrap();
}
#[test]
fn a_driver_driver_match_has_not_reached_never_takes_an_employee_it_gave_someone() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let (date, _) = seed(&db, &id);
    // Driver Match joins "Luis Hernandez" to the employee by a name variant.
    let mut roster = fixtures::fixture_date("America/Los_Angeles", Some(DEMO_DAY)).unwrap();
    roster["employees"][2]["name"] = json!("HERNANDEZ ORTIZ, LUIS");
    db.publish_timecards(&id, &roster).unwrap();
    add_driver(&db, &id, "luis", "Luis Hernandez");
    db.match_drivers(&id).unwrap();
    assert_eq!(
        person(&db, &id, DriverSource::Amazon, "luis"),
        person(&db, &id, DriverSource::Paycom, "E003")
    );
    // A driver arriving with the employee's exact name before Driver Match runs again
    // would match by name; the employee is already someone's.
    add_driver(&db, &id, "late", "Luis Hernandez Ortiz");
    let data = db
        .meal_comparison(&id, &date, "UTC")
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    let driver = |tid: &str| {
        data["drivers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == tid)
            .cloned()
            .unwrap()
    };
    assert_eq!(driver("luis")["paycomCode"], "E003");
    assert_eq!(driver("luis")["matchType"], "name");
    assert_eq!(driver("late")["matchType"], "unmatched");
    assert!(driver("late")["paycomCode"].is_null());
}
#[test]
fn newer_empty_scope_suppresses_stale_meals_and_latest_paycom_period_wins() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let (date, _) = seed(&db, &id);
    let scope = Scope {
        date: date.clone(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "ALL_DRIVERS".into(),
        timezone: "America/Los_Angeles".into(),
    };
    let mut capture = meals::fixture(&scope);
    capture.itineraries[0].meals.clear();
    db.publish_meals(&id, "new-scope", &capture, &scope)
        .unwrap();
    db.collector(&id,cortex::PROVIDER).unwrap().exec("UPDATE meal_publications SET collected_at='2099-01-01T00:00:00.000Z' WHERE job_id='new-scope'",[]).unwrap();
    let data = db
        .meal_comparison(&id, &date, "UTC")
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    assert_eq!(data["rows"].as_array().unwrap().len(), 11);
    assert!(
        data["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["cortex"] == json!([]))
    );
    let mut newer = fixtures::fixture_date("America/Los_Angeles", Some(DEMO_DAY)).unwrap();
    newer["collectedAt"] = json!("2099-01-01T00:00:00Z");
    newer["timecards"] = json!([]);
    db.publish_timecards(&id, &newer).unwrap();
    assert_eq!(
        db.meal_comparison(&id, &date, "UTC")
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["rows"],
        json!([])
    );
}

#[test]
fn ambiguity_includes_employees_without_punches_and_drivers_without_meals() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let (date, _) = seed(&db, &id);
    let paycom = db.collector(&id, paycom::PROVIDER).unwrap();
    paycom
        .exec(
            "UPDATE employees SET name='DEMO DRIVER' WHERE code='E002'",
            [],
        )
        .unwrap();
    let data = db
        .meal_comparison(&id, &date, "UTC")
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    assert_eq!(data["rows"].as_array().unwrap().len(), 12);
    assert_eq!(data["drivers"][0]["matchType"], "unmatched");
    paycom
        .exec(
            "UPDATE employees SET name='Someone Else' WHERE code='E002'",
            [],
        )
        .unwrap();
    db.collector(&id, cortex::PROVIDER).unwrap().exec(
        "INSERT INTO meal_itineraries SELECT publication_id,itinerary_id||'-2',transporter_id||'-2',driver_name,route_code,observed_at,route_complete,delivery_coverage,meal_state FROM meal_itineraries",[]
    ).unwrap();
    let data = db
        .meal_comparison(&id, &date, "UTC")
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    assert_eq!(data["rows"].as_array().unwrap().len(), 12);
    assert_eq!(data["drivers"].as_array().unwrap().len(), 1);
    assert_eq!(data["drivers"][0]["matchType"], "unmatched");
}
