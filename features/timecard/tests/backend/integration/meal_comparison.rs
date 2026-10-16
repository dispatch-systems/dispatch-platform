use dispatch_core::db::{Store, s};
use dispatch_core::testing as common;
use dispatch_cortex::{self as cortex, discovery::Scope, meals};
use dispatch_paycom::fixtures;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// Timecard and both collectors it keeps.
fn install() {
    common::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&dispatch_timecard::FEATURE],
    );
}

/// A day in the demo period of Sep 6 to 19, 2026, within a week of its start.
const DEMO_DAY: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();
fn seed(db: &Store, id: &str) -> (String, String) {
    // A fixed day inside the demo period, so the period's first day has timecards whatever
    // today is: the fixture seeds the seven days up to "today", and the comparison reads
    // the period's first day.
    let mut workforce = fixtures::fixture_date("America/Los_Angeles", Some(DEMO_DAY)).unwrap();
    let date = s(&workforce, "from").to_owned();
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
    let capture = meals::fixture(&scope);
    let transporter = capture.itineraries[0].transporter_id.clone();
    db.publish_meals(id, "job-meals", &capture, &scope).unwrap();
    (date, transporter)
}
fn rows(data: &Value) -> &Vec<Value> {
    data["rows"].as_array().unwrap()
}
// A route whose punches Cortex sent in a shape the meal rules can't read keeps its driver
// on the page with that route's meals unknown, linked to the route, rather than reading as
// a driver who took no meal.
#[test]
fn a_driver_whose_meal_punches_cant_be_read_is_unknown_not_absent() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let (date, transporter) = seed(&db, &id);
    let scope = Scope {
        date: date.clone(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "America/Los_Angeles".into(),
    };
    let mut capture = meals::fixture(&scope);
    let route = &mut capture.itineraries[0];
    route.meals.clear();
    route.delivery_coverage = meals::Coverage::Unavailable;
    route.unreadable = Some("meal_repeat_sequence".into());
    let link = format!(
        "https://logistics.amazon.com{}",
        scope.detail_path(&route.id)
    );
    route.source_url = Some(link.clone());
    db.publish_meals(&id, "unreadable", &capture, &scope)
        .unwrap();
    let data = db
        .meal_comparison(&id, &date, "UTC")
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    // Timecard alone knows nobody's employee, so the driver has a row of their own.
    let row = rows(&data)
        .iter()
        .find(|r| r["id"] == format!("cortex:{transporter}"))
        .unwrap();
    assert_eq!(row["cortex"], json!([]));
    assert_eq!(row["assessment"]["status"], "cortex_unreadable");
    let unknown = &row["cortexUnreadable"][0];
    assert_eq!(
        (
            &unknown["rule"],
            &unknown["sourceUrl"],
            &unknown["itineraryId"]
        ),
        (
            &json!("meal_repeat_sequence"),
            &json!(link),
            &json!("fixture-itinerary")
        )
    );
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
fn meal_period_counts_each_verified_gap_and_only_time_beyond_five_minutes() {
    install();
    let (_root, db, id) = common::bootstrapped();
    for day in ["2026-09-05", "2026-09-19"] {
        let workforce =
            fixtures::fixture_date("America/Los_Angeles", Some(day.parse().unwrap())).unwrap();
        db.publish_timecards(&id, &workforce).unwrap();
    }
    for (date, before, after) in [
        ("2026-09-05", 99, 99), // Outside the period.
        ("2026-09-06", 7, 5),   // Exactly five minutes is excluded.
        ("2026-09-07", 9, 12),
        ("2026-09-08", 8, 6),
        ("2026-09-09", 5, 5),
        ("2026-09-20", 99, 99),
    ] {
        let scope = Scope {
            date: date.into(),
            station: "DEMO1".into(),
            service_area_id: "area-demo".into(),
            provider: "provider-demo".into(),
            timezone: "America/Los_Angeles".into(),
        };
        let mut capture = meals::fixture(&scope);
        let route = &mut capture.itineraries[0];
        route.meals.truncate(1);
        let meal = &mut route.meals[0];
        meal.last_delivery = Some(meal.start - before * 60_000);
        meal.first_delivery = Some(meal.end.unwrap() + after * 60_000);
        db.publish_meals(&id, &format!("period-{date}"), &capture, &scope)
            .unwrap();
    }
    // Timecard alone knows nobody's employee: the driver's meals are their own row's.
    let transporter = meals::fixture(&Scope {
        date: "2026-09-06".into(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "America/Los_Angeles".into(),
    })
    .itineraries[0]
        .transporter_id
        .clone();
    let driver = format!("cortex:{transporter}");
    let result = db
        .meal_period(
            &id,
            driver.as_str(),
            Some("2026-09-07"),
            None,
            "America/Los_Angeles",
        )
        .unwrap();
    assert_eq!(result.period.from, "2026-09-06");
    assert_eq!(result.period.to, "2026-09-19");
    assert_eq!(result.days.len(), 14);
    assert!(
        result
            .days
            .iter()
            .all(|day| day.rows.iter().all(|row| row.source.id == driver))
    );
    assert_eq!(result.gaps.len(), 5);
    assert_eq!(
        result
            .gaps
            .iter()
            .map(|gap| gap.milliseconds / 60_000)
            .collect::<Vec<_>>(),
        vec![7, 9, 12, 8, 6]
    );
    assert_eq!(result.excess_milliseconds, 17 * 60_000);
    // Everyone's counts, as the page shows them, count the same gaps over the same period.
    let everyone = db
        .meal_period_gaps(&id, "2026-09-07", "America/Los_Angeles")
        .unwrap();
    assert_eq!(everyone.period.as_ref(), Some(&result.period));
    assert_eq!(everyone.counts[&driver], 5);
    let previous = db
        .meal_period(
            &id,
            driver.as_str(),
            Some("2026-09-05"),
            None,
            "America/Los_Angeles",
        )
        .unwrap();
    assert_eq!(previous.period.from, "2026-08-23");
    assert_eq!(previous.period.to, "2026-09-05");
    assert_eq!(previous.gaps.len(), 2);
    assert_eq!(
        db.meal_period_gaps(&id, "2026-09-05", "America/Los_Angeles")
            .unwrap()
            .counts[&driver],
        2
    );
    // A day past Paycom's latest period has no period to count over.
    let unpublished = db
        .meal_period_gaps(&id, "2026-09-20", "America/Los_Angeles")
        .unwrap();
    assert!(unpublished.period.is_none() && unpublished.counts.is_empty());
    assert_eq!(
        db.timecard_period(&id, Some("E001"), Some("2026-09-07"), None)
            .unwrap(),
        result.period
    );
    let invalid = dispatch_paycom::timecards::EmployeeTimecardPeriod {
        from: "2026-09-07".into(),
        to: "2026-09-20".into(),
    };
    assert!(
        db.meal_period(&id, driver.as_str(), None, Some(&invalid), "UTC")
            .is_err()
    );
}
