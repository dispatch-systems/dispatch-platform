use common::bootstrapped;
use dispatch_core::db::{Store, s};
use dispatch_core::testing as common;
use dispatch_paycom::{self as paycom, fixtures};
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// Timecard, the Driver Match it joins drivers through, and both collectors it keeps.
fn install() {
    common::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&dispatch_timecard::FEATURE],
    );
}

#[test]
fn demo_periods_follow_the_paycom_cycle_and_never_record_future_punches() {
    for (day, from, to) in [
        ("2026-09-05", "2026-08-23", "2026-09-05"),
        ("2026-09-06", "2026-09-06", "2026-09-19"),
        ("2026-09-19", "2026-09-06", "2026-09-19"),
        ("2026-09-20", "2026-09-20", "2026-10-03"),
        ("2025-12-31", "2025-12-28", "2026-01-10"),
        ("2026-03-08", "2026-03-08", "2026-03-21"),
        ("2028-02-29", "2028-02-20", "2028-03-04"),
    ] {
        let data = fixtures::fixture_date("America/Chicago", Some(day.parse().unwrap())).unwrap();
        assert_eq!(data["from"], from);
        assert_eq!(data["to"], to);
        assert!(data["timecards"].as_array().unwrap().iter().all(|card| {
            s(card, "date") >= from && s(card, "date") <= day && s(card, "date") <= to
        }));
    }
}

#[test]
fn employee_timecards_use_period_order_and_the_latest_revision_within_each_period() {
    install();
    use dispatch_paycom::timecards::EmployeeTimecardPeriod;
    let (_root, db, id) = bootstrapped();
    let period_data = |date: &str, collected: &str, hours: f64| {
        let mut data = fixtures::fixture_date("UTC", Some(date.parse().unwrap())).unwrap();
        data["collectedAt"] = json!(collected);
        data["timecards"][0]["hours"] = json!(hours);
        data
    };
    let old = period_data("2026-08-22", "2026-08-23T00:00:00Z", 5.0);
    let middle = period_data("2026-09-05", "2026-09-06T00:00:00Z", 6.0);
    let current = period_data("2026-09-19", "2026-09-20T00:00:00Z", 7.0);
    for data in [&old, &middle, &current] {
        db.publish_timecards(&id, data).unwrap();
    }
    // A later sync of an older period must update that period, not replace Latest.
    let mut revision = period_data("2026-09-05", "2026-09-21T00:00:00Z", 9.0);
    revision["employees"][0]["name"] = json!("Old employee name");
    db.publish_timecards(&id, &revision).unwrap();
    let latest = db.employee_timecard(&id, "E001", None).unwrap();
    assert_eq!(latest.period.from, "2026-09-06");
    assert_eq!(latest.period.to, "2026-09-19");
    assert_eq!(latest.employee.name, "Avery Morgan");
    assert!(latest.next_period.is_none());
    let previous = db
        .employee_timecard(&id, "E001", latest.previous_period.as_ref())
        .unwrap();
    assert_eq!(previous.period.from, "2026-08-23");
    assert_eq!(previous.period.to, "2026-09-05");
    assert_eq!(previous.next_period, Some(latest.period.clone()));
    assert_eq!(previous.timecards.last().unwrap().card.hours, 9.0);
    let first = db
        .employee_timecard(&id, "E001", previous.previous_period.as_ref())
        .unwrap();
    assert_eq!(first.period.from, "2026-08-09");
    assert_eq!(first.period.to, "2026-08-22");
    let uncollected = db
        .employee_timecard(&id, "E001", first.previous_period.as_ref())
        .unwrap();
    assert_eq!(uncollected.period.from, "2026-07-26");
    assert!(uncollected.collected_at.is_none());
    assert!(uncollected.timecards.is_empty());
    assert_eq!(uncollected.next_period, Some(first.period.clone()));
    assert!(uncollected.previous_period.is_some());
    assert_eq!(first.next_period, Some(previous.period));
    let missing = EmployeeTimecardPeriod {
        from: "2025-01-01".into(),
        to: "2025-01-07".into(),
    };
    assert_eq!(
        db.employee_timecard(&id, "E001", Some(&missing))
            .err()
            .unwrap()
            .code,
        "invalid_period"
    );
    assert_eq!(
        db.employee_timecard(&id, "UNKNOWN", None)
            .err()
            .unwrap()
            .code,
        "employee_not_found"
    );
    // Period history is scoped to the employee, even when their code exists elsewhere.
    revision["employees"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["code"] != "E002");
    revision["timecards"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["employeeCode"] != "E002");
    revision["collectedAt"] = json!("2026-09-22T00:00:00Z");
    db.publish_timecards(&id, &revision).unwrap();
    assert_eq!(
        db.employee_timecard(&id, "E002", None).unwrap().period.to,
        "2026-09-19"
    );
}

#[test]
fn employee_status_filter_applies_before_counting_and_pagination() {
    install();
    let (_root, db, id) = bootstrapped();
    let mut data = fixtures::fixture("UTC").unwrap();
    data["employees"][1]["active"] = json!(false);
    data["employees"][4]["active"] = json!(false);
    db.publish_timecards(&id, &data).unwrap();
    let inactive = db
        .timecard_employees(&id, "", 0, Some(1), false, Some(false))
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    assert_eq!(inactive["total"], 2);
    assert_eq!(inactive["employees"].as_array().unwrap().len(), 1);
    assert_eq!(inactive["employees"][0]["active"], false);
    let next = db
        .timecard_employees(&id, "", 1, Some(1), false, Some(false))
        .map(|value| serde_json::to_value(value).unwrap())
        .unwrap();
    assert_ne!(
        inactive["employees"][0]["code"],
        next["employees"][0]["code"]
    );
    assert_eq!(
        db.timecard_employees(&id, "", 0, Some(100), false, Some(true))
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["total"],
        10
    );
    assert_eq!(
        db.timecard_employees(&id, "Jordan", 0, Some(100), false, Some(false))
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["total"],
        1
    );
    assert_eq!(
        db.timecard_employees(&id, "Jordan", 0, Some(100), false, Some(true))
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["total"],
        0
    );
}

#[test]
fn publication_is_atomic_and_keeps_the_last_successful_dataset() {
    install();
    let (_root, db, id) = bootstrapped();
    let id = id.as_str();
    let data = fixtures::fixture("UTC").unwrap();
    db.publish_timecards(id, &data).unwrap();
    let before = db.employee_timecard(id, "E001", None).unwrap();
    let mut bad = data.clone();
    bad["employees"][1]["code"] = json!("E001");
    assert_eq!(
        db.publish_timecards(id, &bad).unwrap_err().code,
        "duplicate_employee"
    );
    bad = data.clone();
    bad["timecards"][0]["employeeCode"] = json!("unowned");
    assert_eq!(
        db.publish_timecards(id, &bad).unwrap_err().code,
        "timecard_identity_mismatch"
    );
    bad = data.clone();
    bad["timecards"][0]["hours"] = json!(49);
    assert!(db.publish_timecards(id, &bad).is_err());
    bad = data.clone();
    bad["timecards"][0]["date"] = json!("2026-02-30");
    assert!(db.publish_timecards(id, &bad).is_err());
    assert_eq!(db.employee_timecard(id, "E001", None).unwrap(), before);
    let storage = db.collector(id, paycom::PROVIDER).unwrap();
    let current = storage
        .setting("paycom.publicationFingerprint", Value::Null)
        .unwrap();
    let mut historical =
        fixtures::fixture_date("UTC", Some("2020-09-05".parse().unwrap())).unwrap();
    historical["collectedAt"] = json!("2099-01-01T00:00:00.000Z");
    historical["employees"][0]["name"] = json!("Historical name");
    db.publish_timecards(id, &historical).unwrap();
    assert_eq!(
        storage
            .setting("paycom.publicationFingerprint", Value::Null)
            .unwrap(),
        current
    );
    let active: (String,) = storage
        .one_as("SELECT id FROM publications WHERE active=1", [])
        .unwrap()
        .unwrap();
    assert_eq!(active.0, s(&current, "id"));
    assert_eq!(
        db.timecard_employees(id, "", 0, None, false, None)
            .unwrap()
            .total,
        12
    );
    assert_eq!(db.employee_timecard(id, "E001", None).unwrap(), before);
    assert!(
        !db.employee_timecard(
            id,
            "E001",
            Some(&dispatch_paycom::timecards::EmployeeTimecardPeriod {
                from: s(&historical, "from").into(),
                to: s(&historical, "to").into()
            })
        )
        .unwrap()
        .timecards
        .is_empty()
    );
    let mut later = data.clone();
    later["collectedAt"] = json!("2099-01-01T00:00:00.000Z");
    later["employees"].as_array_mut().unwrap().remove(0);
    later["timecards"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["employeeCode"] != "E001");
    db.publish_timecards(id, &later).unwrap();
    assert_eq!(
        db.timecard_employees(id, "", 0, Some(100), false, None)
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["total"],
        11
    );
    assert_eq!(db.employee_timecard(id, "E001", None).unwrap(), before);
}

#[test]
fn timecard_links_publish_with_unchanged_hours_and_are_returned() {
    install();
    let (_root, db, id) = bootstrapped();
    let id = id.as_str();
    let data = fixtures::fixture("UTC").unwrap();
    db.publish_timecards(id, &data).unwrap();
    // Publications from before links were retained return none.
    assert_eq!(
        db.employee_timecard(id, "E001", None).unwrap().timecards[0]
            .card
            .source_url,
        None
    );
    let link = |code: &str| {
        format!(
            "https://paycom.example/v4/cl/web.php/timecard/index?firstrefno={code}&perioddates=P1&formtype=SUMMARY"
        )
    };
    let mut linked = data.clone();
    linked["collectedAt"] = json!("2099-01-01T00:00:00.000Z");
    linked["sources"] = json!(
        data["employees"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| json!({"employeeCode":e["code"],"periodKey":"P1","url":link(s(e,"code"))}))
            .collect::<Vec<_>>()
    );
    for (field, value, code) in [
        (
            "url",
            json!("https://user:secret@paycom.example/"),
            "invalid_source_url",
        ),
        ("url", json!("javascript:alert(1)"), "invalid_source_url"),
        ("employeeCode", json!("unowned"), "timecard_source_mismatch"),
        ("employeeCode", json!("E002"), "timecard_source_mismatch"),
    ] {
        let mut bad = linked.clone();
        bad["sources"][0][field] = value;
        assert_eq!(db.publish_timecards(id, &bad).unwrap_err().code, code);
    }
    // Identical hours still publish: the links are part of the change fingerprint.
    db.publish_timecards(id, &linked).unwrap();
    let paycom = db.collector(id, paycom::PROVIDER).unwrap();
    let count = |table: &str| {
        paycom
            .one(&format!("SELECT count(*) n FROM {table}"), [])
            .unwrap()
            .unwrap()["n"]
            .clone()
    };
    assert_eq!(count("publications"), 2);
    assert_eq!(count("timecard_sources"), 12);
    // An identical repeat only refreshes the collection time.
    linked["collectedAt"] = json!("2099-01-02T00:00:00.000Z");
    db.publish_timecards(id, &linked).unwrap();
    assert_eq!(count("publications"), 2);
    for card in db.employee_timecard(id, "E003", None).unwrap().timecards {
        assert_eq!(card.card.source_url, Some(link("E003")));
    }
    let date = s(&data["timecards"][0], "date");
    let (_, _, rows) = db.daily_timecards_source(id, date).unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        assert_eq!(row["sourceUrl"], json!(link(s(&row, "employeeCode"))));
    }
}

#[test]
fn unchanged_publications_reuse_storage_but_changed_data_and_history_survive() {
    install();
    let (_root, db, id) = bootstrapped();
    let id = id.as_str();
    let mut data = fixtures::fixture("UTC").unwrap();
    db.publish_timecards(id, &data).unwrap();
    let provider = db.collector(id, paycom::PROVIDER).unwrap();
    let first = provider
        .one("SELECT id FROM publications WHERE active=1", [])
        .unwrap()
        .unwrap();
    data["collectedAt"] = json!("2099-01-01T00:00:00.000Z");
    data["employees"].as_array_mut().unwrap().reverse();
    data["timecards"].as_array_mut().unwrap().reverse();
    db.publish_timecards(id, &data).unwrap();
    assert_eq!(
        provider.all("SELECT id FROM publications", []).unwrap(),
        vec![first.clone()]
    );
    assert_eq!(
        db.timecard_employees(id, "", 0, Some(100), false, None)
            .map(|value| serde_json::to_value(value).unwrap())
            .unwrap()["collectedAt"],
        data["collectedAt"]
    );
    data["timecards"][0]["hours"] = json!(7.25);
    data["collectedAt"] = json!("2099-01-02T00:00:00.000Z");
    db.publish_timecards(id, &data).unwrap();
    assert_eq!(
        provider
            .all("SELECT id FROM publications", [])
            .unwrap()
            .len(),
        2
    );
    assert!(
        provider
            .one("SELECT id FROM publications WHERE id=?", [s(&first, "id")])
            .unwrap()
            .is_some()
    );
    // A rollback runtime can publish without maintaining the new fingerprint.
    provider
        .exec(
            "DELETE FROM settings WHERE key='paycom.publicationFingerprint'",
            [],
        )
        .unwrap();
    data["collectedAt"] = json!("2099-01-03T00:00:00.000Z");
    db.publish_timecards(id, &data).unwrap();
    assert_eq!(
        provider
            .all("SELECT id FROM publications", [])
            .unwrap()
            .len(),
        3
    );
}

/// Saves `values` of Timecard's section `id` for the DSP as Settings does, its first save.
fn set(db: &Store, dsp: &str, id: &str, values: Value) {
    let section = (dispatch_timecard::FEATURE.settings.iter())
        .find(|section| section.id == id)
        .unwrap();
    let actor = common::platform_owner(db);
    dispatch_core::settings::save(db, dsp, &actor, section, &values, 0, true).unwrap();
}
/// The DSP's values of Timecard's section `id`.
fn values(db: &Store, dsp: &str, id: &str) -> Value {
    let section = (dispatch_timecard::FEATURE.settings.iter())
        .find(|section| section.id == id)
        .unwrap();
    dispatch_core::settings::read(db, dsp, section).unwrap()
}

#[test]
fn no_driver_department_lists_nobody_on_the_timecard_tab_and_everyone_in_the_roster() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let id = id.as_str();
    db.publish_timecards(id, &fixtures::fixture("UTC").unwrap())
        .unwrap();
    let none = json!({"all_departments": false, "departments": []});
    set(&db, id, "timecard.drivers", none);
    let day = fixtures::fixture("UTC").unwrap()["timecards"][0]["date"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        db.daily_timecards(id, &day, "name", false)
            .unwrap()
            .rows
            .is_empty()
    );
    assert_eq!(
        db.timecard_employees(id, "", 0, Some(100), false, None)
            .unwrap()
            .total,
        12
    );
}

#[test]
fn preferences_kept_in_paycoms_database_move_into_its_settings_once() {
    install();
    let (_root, db, id) = common::bootstrapped();
    let id = id.as_str();
    db.publish_timecards(id, &fixtures::fixture("UTC").unwrap())
        .unwrap();
    // As the last release kept them: an empty filter for all, and keys retired before it.
    let kept = json!({"revision": 4, "history": [], "values": {
        "opening_page": "employees", "rows_per_page": 25, "default_sort": "inDay",
        "columns": ["inDay"], "automatic_sync": true,
        "name_order": "last_first", "department": "", "station": "DEMO1",
        "driver_departments": ["Delivery"],
        "late_da_time": "09:45", "late_da_departments": ["Operations"],
    }});
    let paycom = db.collector(id, paycom::PROVIDER).unwrap();
    paycom.set("paycom.preferences", &kept).unwrap();
    drop(paycom);
    common::before_settings(&db, id).unwrap();
    db.open_collectors(id).unwrap();
    assert_eq!(
        values(&db, id, "timecard.roster"),
        json!({"department": null, "station": "DEMO1", "name_order": "last_first"})
    );
    assert_eq!(
        values(&db, id, "timecard.drivers"),
        json!({"all_departments": false, "departments": ["Delivery"]})
    );
    assert_eq!(
        values(&db, id, "timecard.late_das"),
        json!({"time": "09:45", "departments": ["Operations"]})
    );
    // What Settings saves after stays, however often the DSP opens.
    let section = (dispatch_timecard::FEATURE.settings.iter())
        .find(|section| section.id == "timecard.late_das")
        .unwrap();
    let actor = common::platform_owner(&db);
    let later = json!({"time": "11:00"});
    dispatch_core::settings::save(&db, id, &actor, section, &later, 1, true).unwrap();
    db.open_collectors(id).unwrap();
    assert_eq!(values(&db, id, "timecard.late_das")["time"], "11:00");
}

#[test]
fn employee_and_daily_names_use_first_last_for_search_and_pagination() {
    install();
    let (_root, db, id) = bootstrapped();
    let mut data = fixtures::fixture_date("UTC", Some("2026-09-19".parse().unwrap())).unwrap();
    data["employees"].as_array_mut().unwrap().truncate(3);
    for (employee, name) in data["employees"].as_array_mut().unwrap().iter_mut().zip([
        "ZULU, ANA",
        "ALLEN JR, JUSTIN",
        "de la Cruz, Mateo",
    ]) {
        employee["name"] = json!(name);
    }
    data["timecards"]
        .as_array_mut()
        .unwrap()
        .retain(|card| ["E001", "E002", "E003"].contains(&s(card, "employeeCode")));
    db.publish_timecards(&id, &data).unwrap();
    set(
        &db,
        &id,
        "timecard.roster",
        json!({"name_order": "last_first"}),
    );
    let directory = db
        .timecard_employees(&id, "", 0, Some(2), false, None)
        .unwrap();
    assert_eq!(directory.total, 3);
    assert_eq!(
        directory
            .employees
            .iter()
            .map(|employee| employee.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Ana Zulu", "Justin Allen Jr"]
    );
    let search = db
        .timecard_employees(&id, "Mateo de la Cruz", 0, None, false, None)
        .unwrap();
    assert_eq!(search.employees.len(), 1);
    assert_eq!(search.employees[0].name, "Mateo de la Cruz");
    assert_eq!(
        db.employee_timecard(&id, "E002", None)
            .unwrap()
            .employee
            .name,
        "Justin Allen Jr"
    );
    let daily = db
        .daily_timecards(&id, "2026-09-19", "name", false)
        .unwrap();
    assert_eq!(
        daily
            .rows
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Ana Zulu", "Justin Allen Jr", "Mateo de la Cruz"]
    );
}
