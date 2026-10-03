//! Range reads keep historical selection, overlays and complete identity context.
#[path = "../../../../../core/db/tests/support/common.rs"]
mod common;
use dispatch_backend::{
    collectors::{
        cortex::{discovery::Scope, meals},
        paycom::{self, fixtures},
    },
    db::s,
};
use serde_json::json;

#[test]
fn daily_ranges_select_each_days_latest_publication_and_employee_overlay() {
    let (_root, db, id) = common::bootstrapped();
    let fixture = |day: &str, collected: &str| {
        let mut data = fixtures::fixture_date("UTC", Some(day.parse().unwrap())).unwrap();
        data["collectedAt"] = json!(collected);
        data
    };
    let old = fixture("2026-09-05", "2099-01-01T00:00:00Z");
    db.publish(&id, &old).unwrap();
    let current = fixture("2026-09-19", "2099-01-02T00:00:00Z");
    db.publish(&id, &current).unwrap();
    let mut revision = old.clone();
    revision["collectedAt"] = json!("2099-01-03T00:00:00Z");
    for card in revision["timecards"].as_array_mut().unwrap() {
        if card["employeeCode"] == "E002" {
            card["hours"] = json!(6.25);
        }
    }
    db.publish(&id, &revision).unwrap();
    let mut sync = revision.clone();
    sync["employees"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["code"] == "E002");
    sync["timecards"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["employeeCode"] == "E002");
    for card in sync["timecards"].as_array_mut().unwrap() {
        card["hours"] = json!(4.25);
    }
    db.collector(&id, paycom::PROVIDER)
        .unwrap()
        .exec(
            "INSERT INTO employee_timecard_syncs VALUES ('E002',?,?,?,?)",
            rusqlite::params![
                s(&sync, "from"),
                s(&sync, "to"),
                "2099-01-04T00:00:00Z",
                sync.to_string()
            ],
        )
        .unwrap();
    let selected = ["E002".to_owned()];
    let range = db
        .daily_range(
            &id,
            "2026-09-04",
            "2026-09-20",
            "name",
            false,
            Some(&selected),
        )
        .unwrap();
    assert_eq!(range.len(), 17);
    assert_eq!(range["2026-09-04"].rows[0].card.hours, 4.25);
    assert_eq!(range["2026-09-05"].rows[0].card.hours, 4.25);
    assert_eq!(range["2026-09-19"].rows[0].card.hours, 8.0);
    assert_eq!(
        range["2026-09-05"].collected_at.as_deref(),
        Some("2099-01-03T00:00:00Z")
    );
    assert!(!range["2026-09-20"].available);
    assert!(range["2026-09-20"].rows.is_empty());
    assert!(
        range
            .values()
            .all(|day| day.rows.iter().all(|r| r.card.employee_code == "E002"))
    );
    // Source coverage is independent of whether a selected employee has rows.
    let absent = ["not-listed".to_owned()];
    let missing = db
        .daily_range(
            &id,
            "2026-09-04",
            "2026-09-05",
            "name",
            false,
            Some(&absent),
        )
        .unwrap();
    assert!(
        missing
            .values()
            .all(|day| day.available && day.rows.is_empty())
    );
    // Only a newer full capture of that period supersedes the employee overlay.
    revision["collectedAt"] = json!("2099-01-05T00:00:00Z");
    for card in revision["timecards"].as_array_mut().unwrap() {
        if card["employeeCode"] == "E002" {
            card["hours"] = json!(7.25);
        }
    }
    db.publish(&id, &revision).unwrap();
    assert_eq!(
        db.daily_range(
            &id,
            "2026-09-04",
            "2026-09-05",
            "name",
            false,
            Some(&selected)
        )
        .unwrap()["2026-09-05"]
            .rows[0]
            .card
            .hours,
        7.25
    );
}

#[test]
fn meal_ranges_keep_full_name_context_and_newest_observation_per_day() {
    let (_root, db, id) = common::bootstrapped();
    let mut roster = fixtures::fixture_date("UTC", Some("2026-09-05".parse().unwrap())).unwrap();
    roster["employees"][1]["name"] = json!("DOE, ALEX");
    roster["employees"][2]["name"] = json!("DOE, ALEX");
    db.publish(&id, &roster).unwrap();
    let scope = |date: &str, provider: &str, timezone: &str| Scope {
        date: date.into(),
        station: "TST1".into(),
        service_area_id: "area-1".into(),
        provider: provider.into(),
        timezone: timezone.into(),
    };
    let first = scope("2026-09-04", "provider-1", "UTC");
    let mut capture = meals::fixture(&first);
    capture.started_at -= 3000;
    capture.finished_at -= 3000;
    capture.itineraries[0].observed_at -= 3000;
    capture.itineraries[0].transporter_id = "driver-1".into();
    capture.itineraries[0].driver = "Alex Doe".into();
    let url = format!(
        "https://logistics.amazon.com{}",
        first.detail_path("fixture-itinerary")
    );
    capture.itineraries[0].source_url = Some(url.clone());
    let mut duplicate = capture.itineraries[0].clone();
    duplicate.id = "duplicate-name".into();
    duplicate.transporter_id = "driver-2".into();
    duplicate.meals.clear();
    duplicate.source_url = None;
    capture.itineraries.push(duplicate);
    db.publish_meals(&id, "first-day", &capture, &first)
        .unwrap();
    let second = scope("2026-09-05", "provider-1", "America/Chicago");
    let mut capture = meals::fixture(&second);
    capture.started_at -= 2000;
    capture.finished_at -= 2000;
    capture.itineraries[0].observed_at -= 2000;
    capture.itineraries[0].transporter_id = "driver-1".into();
    capture.itineraries[0].driver = "Alex Doe".into();
    db.publish_meals(&id, "second-day", &capture, &second)
        .unwrap();
    let narrower = scope("2026-09-05", "provider-2", "America/Chicago");
    let mut no_meal = capture.clone();
    no_meal.scope = narrower.clone();
    no_meal.started_at += 1000;
    no_meal.finished_at += 1000;
    no_meal.itineraries[0].observed_at += 1000;
    no_meal.itineraries[0].meals.clear();
    db.publish_meals(&id, "newer-empty", &no_meal, &narrower)
        .unwrap();
    let selected = ["paycom:E002".to_owned(), "cortex:driver-1".to_owned()];
    let range = db
        .meal_comparisons(&id, "2026-09-04", "2026-09-06", "UTC", Some(&selected))
        .unwrap();
    assert_eq!(range.len(), 3);
    let first = &range["2026-09-04"];
    let meal = first
        .rows
        .iter()
        .find(|r| r.source.id == "cortex:driver-1")
        .unwrap();
    assert_eq!(meal.source.cortex[0].source_url.as_ref(), Some(&url));
    assert!(
        meal.source.cortex[0]
            .last_delivery_url
            .as_ref()
            .unwrap()
            .ends_with("selectedStopId=11")
    );
    let card = first
        .rows
        .iter()
        .find(|r| r.source.id == "paycom:E002")
        .unwrap();
    assert!(
        card.source.cortex.is_empty(),
        "filtering cards must not make duplicate names unique"
    );
    assert_eq!(first.drivers.len(), 1);
    assert!(first.drivers[0].paycom_code.is_none());
    let second = &range["2026-09-05"];
    assert_eq!(second.timezone, "America/Chicago");
    assert_eq!(second.cortex_publications.len(), 2);
    assert!(
        second.rows.iter().all(|r| r.source.cortex.is_empty()),
        "newer meal-free itinerary masks older scopes"
    );
    assert_eq!(range["2026-09-06"].timezone, "America/Chicago");
    assert!(range["2026-09-06"].cortex_publications.is_empty());
}
