//! Driver Match: every ID the collections hold gets one person's code, certain names join
//! on their own, and the rest wait for a decision that merges, splits or keeps apart.
mod common;
use dispatch_backend::{
    accounts::{Auth, Context},
    contracts::{DriverEventKind, DriverMatch, DriverSource, DriverStatus, DriverStrength},
    db::{Store, s},
    driver_match::valid_code,
    meals::{self, Scope},
    workforce,
};
use serde_json::{Value, json};

/// A day inside the fixture's pay period, so its timecards exist whatever today is.
const DAY: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2026, 9, 12).unwrap();

/// A DSP whose Paycom roster and Amazon meal breaks name the same people in different
/// ways: a short form, a two-part surname, an exact match, a driver Paycom lacks and an
/// office employee. `saved` stands for links made on the meal-break page.
fn ready(saved: Option<Value>) -> (tempfile::TempDir, Store, String, Context) {
    let (root, db, id) = common::bootstrapped();
    db.enable_all_features(&id).unwrap();
    if let Some(saved) = saved {
        db.dsp(&id)
            .unwrap()
            .set("employees.provider_links", &saved)
            .unwrap();
    }
    let mut roster = workforce::fixture_date("UTC", Some(DAY)).unwrap();
    for (code, name) in [
        ("E002", "REYES, ANTONIO"),
        ("E003", "HERNANDEZ ORTIZ, LUIS"),
        ("E005", "OKAFOR, SHANICE"),
    ] {
        let employee = roster["employees"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["code"] == code)
            .unwrap();
        employee["name"] = json!(name);
    }
    db.publish(&id, &roster).unwrap();
    let scope = Scope {
        date: s(&roster, "from").to_owned(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "UTC".into(),
    };
    let mut capture = meals::fixture(&scope);
    let base = capture.itineraries.remove(0);
    for (transporter, name) in [
        ("TREYES", "Tony Reyes"),
        ("TLUIS", "Luis Hernandez"),
        ("TTAYLOR", "Taylor Brooks"),
        ("TSHAY", "Shay Okafor"),
        ("TDMITRI", "Dmitri Volkov"),
    ] {
        let mut itinerary = base.clone();
        itinerary.id = format!("itinerary-{transporter}");
        itinerary.transporter_id = transporter.into();
        itinerary.driver = name.into();
        capture.itineraries.push(itinerary);
    }
    db.publish_meals(&id, "job-meals", &capture, &scope)
        .unwrap();
    let owner = db
        .platform
        .one("SELECT id FROM users LIMIT 1", [])
        .unwrap()
        .unwrap();
    let auth = Auth {
        user: serde_json::from_value(
            json!({"id":owner["id"],"email":"","firstName":"","lastName":"","platformOwner":true}),
        )
        .unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
    };
    let context = db.context(&auth, &id, "driver_match.manage").unwrap();
    (root, db, id, context)
}
/// The code holding an ID, in a Driver Match answer.
fn code(result: &DriverMatch, source: DriverSource, id: &str) -> String {
    result
        .drivers
        .iter()
        .find(|d| d.ids.iter().any(|i| i.source == source && i.id == id))
        .map(|d| d.code.clone())
        .unwrap_or_else(|| panic!("{id} has no person"))
}
fn status(result: &DriverMatch, code: &str) -> DriverStatus {
    result
        .drivers
        .iter()
        .find(|d| d.code == code)
        .unwrap()
        .status
}
/// The subject the platform's activity log shows for the newest event of `action`.
fn shown(db: &Store, action: &str) -> String {
    db.audit_page(&dispatch_backend::db::AuditQuery::default())
        .unwrap()
        .events
        .into_iter()
        .find(|e| e.action == action)
        .and_then(|e| e.target)
        .unwrap_or_else(|| panic!("no {action}"))
}
const PAYCOM: DriverSource = DriverSource::Paycom;
const AMAZON: DriverSource = DriverSource::Amazon;

#[test]
fn every_id_gets_one_code_and_only_certain_names_join_on_their_own() {
    let (_root, db, id, context) = ready(None);
    // Twelve employees and five drivers.
    assert_eq!(db.match_drivers(&id).unwrap(), 17);
    let result = db.driver_match(&id).unwrap();
    assert_eq!(result.drivers.len(), 15);
    assert!(result.checked_at.is_some());
    assert!(result.drivers.iter().all(|d| valid_code(&d.code)));
    // Every code is listed once on the platform.
    assert_eq!(
        db.platform
            .count("SELECT count(*) FROM driver_codes WHERE dsp_id=?", [&id])
            .unwrap(),
        15
    );
    let taylor = code(&result, PAYCOM, "E004");
    assert_eq!(taylor, code(&result, AMAZON, "TTAYLOR"));
    assert_eq!(status(&result, &taylor), DriverStatus::Matched);
    let luis = code(&result, PAYCOM, "E003");
    assert_eq!(luis, code(&result, AMAZON, "TLUIS"));
    assert_eq!(status(&result, &luis), DriverStatus::Variant);
    // A short form is suggested, never joined by itself.
    let antonio = code(&result, PAYCOM, "E002");
    let tony = code(&result, AMAZON, "TREYES");
    assert_ne!(antonio, tony);
    assert_eq!(status(&result, &antonio), DriverStatus::Review);
    // Nobody is office staff until the DSP says which departments drive.
    assert_eq!(
        status(&result, &code(&result, PAYCOM, "E001")),
        DriverStatus::PaycomOnly
    );
    assert_eq!(
        status(&result, &code(&result, AMAZON, "TDMITRI")),
        DriverStatus::AmazonOnly
    );
    assert_eq!(result.counts.review, 2);
    assert_eq!(result.counts.office, 0);
    assert_eq!(result.counts.amazon_only, 1);
    let reyes = result
        .review
        .iter()
        .find(|pair| pair.paycom.code == antonio)
        .unwrap();
    assert_eq!(reyes.amazon.code, tony);
    assert_eq!(reyes.strength, DriverStrength::Strong);
    // The Timecard settings' driver departments set the rest apart as office staff.
    let mut values = db.preference_values(&id).unwrap()["values"].clone();
    values["driver_departments"] = json!(["Delivery"]);
    db.save_preferences(&id, &context.auth.user.id, 0, &values)
        .unwrap();
    let staffed = db.driver_match(&id).unwrap();
    assert_eq!(
        status(&staffed, &code(&staffed, PAYCOM, "E001")),
        DriverStatus::Office
    );
    assert_eq!(staffed.counts.office, 1);
    assert_eq!(staffed.counts.drivers, staffed.counts.all - 1);
    // Nothing new the second time, and nobody's code changes.
    assert_eq!(db.match_drivers(&id).unwrap(), 0);
    let again = db.driver_match(&id).unwrap();
    assert_eq!(code(&again, PAYCOM, "E002"), antonio);
    assert_eq!(code(&again, AMAZON, "TLUIS"), luis);
}

#[test]
fn a_merge_makes_one_person_and_the_old_code_still_finds_them() {
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    let after = db.merge_drivers(&context, &tony, &antonio).unwrap();
    assert_eq!(code(&after, AMAZON, "TREYES"), antonio);
    assert_eq!(status(&after, &antonio), DriverStatus::Confirmed);
    assert_eq!(after.counts.review, 1);
    assert_eq!(after.drivers.len(), before.drivers.len() - 1);
    let details = db.driver_details(&id, &tony).unwrap();
    assert_eq!(details.driver.code, antonio);
    assert_eq!(details.merged_codes, std::slice::from_ref(&tony));
    let merged = details
        .history
        .iter()
        .find(|e| e.kind == DriverEventKind::Merged)
        .unwrap();
    assert_eq!(merged.code.as_deref(), Some(tony.as_str()));
    assert_eq!(merged.actor.as_deref(), Some("Platform support"));
    assert_eq!(details.days.len(), 14);
    // Merging what is already merged is refused, as the page is out of date.
    let stale = db.merge_drivers(&context, &tony, &antonio).unwrap_err();
    assert_eq!(stale.code, "driver_changed");
    let merge = db
        .platform
        .one("SELECT * FROM audit WHERE action='driver_match.merged'", [])
        .unwrap()
        .unwrap();
    assert_eq!(s(&merge, "dsp_id"), id);
    assert_eq!(s(&merge, "detail"), format!("{tony} into {antonio}"));
    // The entry names codes: what a collection says about someone stays in its database.
    let data: serde_json::Value = serde_json::from_str(s(&merge, "data")).unwrap();
    assert_eq!(data["target"], format!("{tony} and {antonio}"));
    // The log reads who they are from the collections as it is shown.
    assert_eq!(shown(&db, "driver_match.merged"), "Antonio Reyes");
}

#[test]
fn people_split_or_kept_apart_are_not_suggested_again() {
    let (_root, db, id, context) = ready(None);
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    db.merge_drivers(&context, &tony, &antonio).unwrap();
    let split = db
        .split_driver(&context, &antonio, AMAZON, "TREYES")
        .unwrap();
    let fresh = code(&split, AMAZON, "TREYES");
    assert!(fresh != antonio && fresh != tony && valid_code(&fresh));
    assert!(split.review.iter().all(|pair| pair.paycom.code != antonio));
    let refused = db
        .split_driver(&context, &fresh, AMAZON, "TREYES")
        .unwrap_err();
    assert_eq!(refused.code, "driver_single_id");
    let (shanice, shay) = (code(&split, PAYCOM, "E005"), code(&split, AMAZON, "TSHAY"));
    let apart = db.keep_drivers_apart(&context, &shay, &shanice).unwrap();
    assert!(apart.review.is_empty());
    assert_eq!(status(&apart, &shanice), DriverStatus::PaycomOnly);
    let history = db.driver_details(&id, &shay).unwrap().history;
    assert!(
        history.iter().any(
            |e| e.kind == DriverEventKind::Apart && e.code.as_deref() == Some(shanice.as_str())
        )
    );
    assert_eq!(shown(&db, "driver_match.split"), "Tony Reyes");
    assert_eq!(
        shown(&db, "driver_match.kept_apart"),
        "Shay Okafor and Shanice Okafor"
    );
}

#[test]
fn someone_only_amazon_saw_long_ago_has_left() {
    let (_root, db, id, _) = ready(None);
    let old = Scope {
        date: (DAY - chrono::Duration::days(40)).to_string(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "UTC".into(),
    };
    let mut capture = meals::fixture(&old);
    capture.itineraries.truncate(1);
    let gone = &mut capture.itineraries[0];
    gone.id = "itinerary-TGONE".into();
    gone.transporter_id = "TGONE".into();
    gone.driver = "Morgan Ellis".into();
    db.publish_meals(&id, "job-old-meals", &capture, &old)
        .unwrap();
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let gone = code(&result, AMAZON, "TGONE");
    assert_eq!(status(&result, &gone), DriverStatus::Former);
    // Seen lately, Dmitri still waits for someone to find them in Paycom.
    let dmitri = code(&result, AMAZON, "TDMITRI");
    assert_eq!(status(&result, &dmitri), DriverStatus::AmazonOnly);
    let counts = &result.counts;
    assert_eq!(counts.former, 1);
    assert_eq!(counts.drivers, counts.all - counts.office - counts.former);
    // Paycom's roster lists everyone else, so nobody else has left.
    assert!(
        result
            .drivers
            .iter()
            .all(|d| d.code == gone || d.status != DriverStatus::Former)
    );
}

#[test]
fn links_saved_on_the_meal_break_page_are_kept() {
    let (_root, db, id, _) = ready(Some(json!({
        "revision": 2,
        "links": [{"id":"employee_1","cortexId":"TREYES","paycomCode":"E002"}],
        "separate": ["TTAYLOR"],
    })));
    db.match_drivers(&id).unwrap();
    let result = db.driver_match(&id).unwrap();
    let antonio = code(&result, PAYCOM, "E002");
    assert_eq!(antonio, code(&result, AMAZON, "TREYES"));
    assert_eq!(status(&result, &antonio), DriverStatus::Confirmed);
    // Kept apart from Paycom, Taylor is neither joined nor suggested.
    let taylor = code(&result, AMAZON, "TTAYLOR");
    assert_ne!(taylor, code(&result, PAYCOM, "E004"));
    assert_eq!(status(&result, &taylor), DriverStatus::AmazonOnly);
}

#[test]
fn the_permission_exists_only_while_the_feature_is_on() {
    let (_root, db, id, context) = ready(None);
    db.set_feature(&id, "driver_match", false, &context.auth.user.id)
        .unwrap();
    let refused = db.context(&context.auth, &id, "driver_match.manage").err();
    assert_eq!(
        refused.map(|e| e.code).as_deref(),
        Some("permission_denied")
    );
}

#[test]
fn the_previous_release_keeps_every_link_after_a_decision() {
    // A link saved for a driver whose data has since gone, beside one Driver Match knows.
    let (_root, db, id, context) = ready(Some(json!({
        "revision": 4,
        "links": [{"id":"employee_1","cortexId":"TGONE","paycomCode":"E007"}],
        "separate": ["TDMITRI"],
    })));
    db.match_drivers(&id).unwrap();
    let before = db.driver_match(&id).unwrap();
    let (antonio, tony) = (
        code(&before, PAYCOM, "E002"),
        code(&before, AMAZON, "TREYES"),
    );
    db.merge_drivers(&context, &tony, &antonio).unwrap();
    let legacy = db
        .dsp(&id)
        .unwrap()
        .setting("employees.provider_links", Value::Null)
        .unwrap();
    assert_eq!(legacy["revision"], 5);
    let pairs: Vec<(&str, &str)> = legacy["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (s(l, "cortexId"), s(l, "paycomCode")))
        .collect();
    assert_eq!(pairs, [("TREYES", "E002"), ("TGONE", "E007")]);
    assert_eq!(legacy["separate"], json!(["TDMITRI"]));
}
