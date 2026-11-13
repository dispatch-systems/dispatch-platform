use common::bootstrapped;
use dispatch_core::db::{Store, s};
use dispatch_core::testing as common;
use dispatch_paycom::{self as paycom, fixtures};
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};

/// Timecard, the Driver Match it joins drivers through, and both collectors it keeps.
fn install() {
    super::install();
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
    // A failure after inline rows are written rolls the entire publication back.
    let snapshot = || {
        [
            "publications",
            "employees",
            "timecards",
            "timecard_sources",
            "paycom_payloads",
            "paycom_timecard_payload_refs",
            "paycom_people",
            "paycom_people_workdays",
            "paycom_people_collected_days",
            "paycom_people_state",
            "settings",
        ]
        .into_iter()
        .map(|table| {
            storage
                .all(&format!("SELECT * FROM {table} ORDER BY 1"), [])
                .unwrap()
        })
        .collect::<Vec<_>>()
    };
    let original = snapshot();
    storage
        .0
        .execute_batch(
            "CREATE TRIGGER reject_timecard BEFORE INSERT ON timecards
         WHEN EXISTS(SELECT 1 FROM timecards WHERE publication_id=NEW.publication_id)
         BEGIN SELECT RAISE(ABORT,'fixture_card_failure'); END;",
        )
        .unwrap();
    let mut rejected = data.clone();
    rejected["timecards"][0]["punches"][0]["in"] = json!("07:53");
    assert!(db.publish_timecards(id, &rejected).is_err());
    assert_eq!(snapshot(), original);
    assert_eq!(db.employee_timecard(id, "E001", None).unwrap(), before);
    storage
        .0
        .execute_batch("DROP TRIGGER reject_timecard;")
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
    assert_eq!(count("paycom_payloads"), 0);
    // An identical repeat only refreshes the collection time.
    linked["collectedAt"] = json!("2099-01-02T00:00:00.000Z");
    db.publish_timecards(id, &linked).unwrap();
    assert_eq!(count("publications"), 2);
    db.publish_timecards(id, &linked).unwrap();
    assert_eq!(
        paycom
            .count(
                "SELECT count(*) FROM paycom_timecard_payload_refs WHERE employee_code='E003'
         AND publication_id=(SELECT id FROM publications WHERE active=1)",
                [],
            )
            .unwrap(),
        0,
        "fingerprint reuse must not reconcile incoming bytes into stored history"
    );
    for card in db.employee_timecard(id, "E003", None).unwrap().timecards {
        assert_eq!(card.card.source_url, Some(link("E003")));
    }
    let date = s(&data["timecards"][0], "date");
    let rows = db.daily_timecards(id, date, "name", false).unwrap().rows;
    assert_eq!(rows.len(), 12);
    for row in rows {
        assert_eq!(row.card.source_url, Some(link(&row.card.employee_code)));
    }
    linked["employees"][0]["name"] = json!("Retained Actor");
    linked["collectedAt"] = json!("2099-01-03T00:00:00.000Z");
    db.publish_timecards(id, &linked).unwrap();
    assert_eq!(count("publications"), 3);
    assert_eq!(count("paycom_payloads"), 0);
    assert_eq!(
        db.employee_timecard(id, "E001", None)
            .unwrap()
            .employee
            .name,
        "Retained Actor"
    );
}

#[test]
fn publications_keep_every_inline_punch_without_pool_or_reference_rows() {
    install();
    let (_root, store, id) = bootstrapped();
    let mut data = fixtures::fixture_date("UTC", Some("2026-09-19".parse().unwrap())).unwrap();
    data["timecards"][0]["punches"] = json!([]);
    data["timecards"][0]["hours"] = json!(0.0);
    data["timecards"][0]["status"] = json!("No punches");
    store.publish_timecards(&id, &data).unwrap();
    let db = store.collector(&id, paycom::PROVIDER).unwrap();
    for table in [
        "paycom_payloads",
        "paycom_timecard_payload_refs",
        "paycom_employee_sync_payload_refs",
    ] {
        assert_eq!(
            db.count(&format!("SELECT count(*) FROM {table}"), [])
                .unwrap(),
            0,
            "{table} must stay empty after publication"
        );
    }
    let keeper = dispatch_timecard::FEATURE
        .keeps
        .iter()
        .find(|keeper| keeper.keeps() == "paycom.collect")
        .unwrap();
    let published = keeper.published(&db).unwrap();
    assert_eq!(published.len(), data["timecards"].as_array().unwrap().len());
    let range = store
        .daily_timecards_range(&id, s(&data, "from"), s(&data, "to"), "name", false, None)
        .unwrap();
    for employee in data["employees"].as_array().unwrap() {
        let detail = store
            .employee_timecard(&id, s(employee, "code"), None)
            .unwrap();
        let expected: Vec<_> = data["timecards"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| card["employeeCode"] == employee["code"])
            .collect();
        assert_eq!(detail.timecards.len(), expected.len());
        for expected in expected {
            let punches: Vec<dispatch_timecard::Punch> =
                serde_json::from_value(expected["punches"].clone()).unwrap();
            let actual = detail
                .timecards
                .iter()
                .find(|card| card.card.date == expected["date"])
                .unwrap();
            assert_eq!(actual.card.punches, punches);
            let daily = range[s(expected, "date")]
                .rows
                .iter()
                .find(|row| row.card.employee_code == employee["code"])
                .unwrap();
            assert_eq!(daily.card.punches, punches);
            let probe = published
                .iter()
                .find(|card| {
                    card["employeeCode"] == employee["code"] && card["date"] == expected["date"]
                })
                .unwrap();
            assert_eq!(probe["punches"], expected["punches"]);
            let inline = db
                .one(
                    "SELECT punches FROM timecards WHERE employee_code=? AND date=?",
                    [s(employee, "code"), s(expected, "date")],
                )
                .unwrap()
                .unwrap();
            assert_eq!(inline["punches"], expected["punches"].to_string());
        }
    }
}

#[test]
fn unchanged_publications_reuse_storage_but_changed_data_and_history_survive() {
    install();
    let (_root, db, id) = bootstrapped();
    let id = id.as_str();
    let mut data = fixtures::fixture_date("UTC", Some("2026-09-19".parse().unwrap())).unwrap();
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
    for table in [
        "paycom_payloads",
        "paycom_timecard_payload_refs",
        "paycom_employee_sync_payload_refs",
    ] {
        assert_eq!(
            provider
                .count(&format!("SELECT count(*) FROM {table}"), [])
                .unwrap(),
            0
        );
    }
    let date = "2026-09-13";
    let card = data["timecards"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|card| card["employeeCode"] == "E002" && card["date"] == date)
        .unwrap();
    card["punches"][0]["in"] = json!("07:59");
    let inline = card["punches"].to_string();
    let employee = data["employees"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|employee| employee["code"] == "E002")
        .unwrap();
    employee["name"] = json!("Changed Name");
    data["sources"] = json!([{"employeeCode":"E002","periodKey":"P1",
        "url":"https://paycom.example/timecard/P1"}]);
    data["collectedAt"] = json!("2099-01-04T00:00:00.000Z");
    db.publish_timecards(id, &data).unwrap();
    assert_eq!(
        provider
            .count("SELECT count(*) FROM paycom_payloads", [])
            .unwrap(),
        0
    );
    assert_eq!(
        provider
            .count("SELECT count(*) FROM publications", [])
            .unwrap(),
        4
    );
    let active: (String,) = provider
        .one_as("SELECT id FROM publications WHERE active=1", [])
        .unwrap()
        .unwrap();
    assert_eq!(provider.one(
        "SELECT punches FROM timecards WHERE publication_id=? AND employee_code='E002' AND date=?",
        [&active.0, date],
    ).unwrap().unwrap()["punches"], inline);
    // Fixture for the future stored format; product writers never emit this marker.
    provider
        .transaction(|| {
            // Explicit old-build fixture: canonical v1 bodies and sidecar references.
            let mut refs = Vec::new();
            for mut row in provider.all("SELECT publication_id,employee_code,date,punches FROM timecards", [])? {
                let punches: Vec<std::collections::BTreeMap<String, Value>> =
                    serde_json::from_str(s(&row, "punches"))?;
                let body = serde_json::to_string(&punches)?;
                let digest = dispatch_core::foundation::crypto::sha(format!("paycom.punches.v1\0{body}"));
                provider.exec(
                    "INSERT INTO paycom_payloads VALUES ('paycom.punches.v1',?,?) ON CONFLICT DO NOTHING",
                    rusqlite::params![digest, body],
                )?;
                row["format"] = json!("paycom.punches.v1");
                row["digest"] = json!(digest);
                refs.push(row);
            }
            provider.exec(
                r#"UPDATE timecards SET punches='{"$dispatchPayload":"paycom.punches.v1"}'"#,
                [],
            )?;
            for row in refs {
                provider.exec(
                    "INSERT INTO paycom_timecard_payload_refs
                 (publication_id,employee_code,date,format,digest) VALUES (?,?,?,?,?)",
                    rusqlite::params![
                        s(&row, "publication_id"),
                        s(&row, "employee_code"),
                        s(&row, "date"),
                        s(&row, "format"),
                        s(&row, "digest")
                    ],
                )?;
            }
            Ok(())
        })
        .unwrap();
    let detail = db.employee_timecard(id, "E002", None).unwrap();
    assert_eq!(detail.employee.name, "Changed Name");
    let card = &detail
        .timecards
        .iter()
        .find(|c| c.card.date == date)
        .unwrap()
        .card;
    assert_eq!(card.punches[0].clock_in.as_deref(), Some("07:59"));
    assert_eq!((card.hours, card.status.as_str()), (8.0, "Complete"));
    assert_eq!(
        card.source_url.as_deref(),
        Some("https://paycom.example/timecard/P1")
    );
    let rows = db.daily_timecards(id, date, "name", false).unwrap().rows;
    let row = rows
        .iter()
        .find(|r| r.card.employee_code == "E002")
        .unwrap();
    assert_eq!(row.card.punches[0].clock_in.as_deref(), Some("07:59"));
    assert_eq!(row.name, "Changed Name");
    let selected = ["E002".to_owned()];
    let range = db
        .daily_timecards_range(id, date, "2026-09-14", "name", false, Some(&selected))
        .unwrap();
    assert_eq!(
        range[date].rows[0].card.punches[0].clock_in.as_deref(),
        Some("07:59")
    );
    let keeper = dispatch_timecard::FEATURE
        .keeps
        .iter()
        .find(|k| k.keeps() == "paycom.collect")
        .unwrap();
    let probe = keeper.published(&provider).unwrap();
    assert_eq!(
        probe
            .iter()
            .find(|c| c["employeeCode"] == "E002" && c["date"] == date)
            .unwrap()["punches"][0]["in"],
        "07:59"
    );
    // A shared body is protected while any row (including inactive history) needs it.
    assert!(
        provider
            .exec(
                "DELETE FROM paycom_payloads WHERE digest IN
         (SELECT digest FROM paycom_timecard_payload_refs)",
                [],
            )
            .is_err()
    );
    // Old code can update hours/status without invalidating a punch-only reference.
    provider
        .exec(
            "UPDATE timecards SET hours=7.5,status='Missing punch'
         WHERE publication_id=? AND employee_code='E002' AND date=?",
            [&active.0, date],
        )
        .unwrap();
    let changed = db.employee_timecard(id, "E002", None).unwrap();
    let changed = &changed
        .timecards
        .iter()
        .find(|c| c.card.date == date)
        .unwrap()
        .card;
    assert_eq!(
        (changed.hours, changed.status.as_str()),
        (7.5, "Missing punch")
    );
    assert_eq!(changed.punches[0].clock_in.as_deref(), Some("07:59"));
    // A rollback writer's changed inline bytes immediately supersede the old sidecar.
    provider.exec(
        "UPDATE timecards SET punches=? WHERE publication_id=? AND employee_code='E002' AND date=?",
        rusqlite::params![r#"[{"in":"07:57","out":"16:30","hours":8}]"#,active.0,date],
    ).unwrap();
    assert_eq!(
        provider
            .count(
                "SELECT count(*) FROM paycom_timecard_payload_refs
         WHERE publication_id=? AND employee_code='E002' AND date=?",
                [&active.0, date],
            )
            .unwrap(),
        0
    );
    assert_eq!(
        db.employee_timecard(id, "E002", None)
            .unwrap()
            .timecards
            .iter()
            .find(|c| c.card.date == date)
            .unwrap()
            .card
            .punches[0]
            .clock_in
            .as_deref(),
        Some("07:57")
    );
    provider
        .exec(
            "INSERT OR REPLACE INTO timecards VALUES (?,'E004',?,0,'No punches','[]')",
            [&active.0, date],
        )
        .unwrap();
    assert_eq!(
        provider
            .count(
                "SELECT count(*) FROM paycom_timecard_payload_refs
         WHERE publication_id=? AND employee_code='E004' AND date=?",
                [&active.0, date],
            )
            .unwrap(),
        0
    );
    assert!(
        db.employee_timecard(id, "E004", None)
            .unwrap()
            .timecards
            .iter()
            .find(|c| c.card.date == date)
            .unwrap()
            .card
            .punches
            .is_empty()
    );
    // An ordinary legacy insert is still readable after deleting its original row.
    provider
        .exec(
            "DELETE FROM timecards WHERE publication_id=? AND employee_code='E002' AND date=?",
            [&active.0, date],
        )
        .unwrap();
    provider
        .exec(
            "INSERT INTO timecards VALUES (?,'E002',?,0,'No punches','[]')",
            [&active.0, date],
        )
        .unwrap();
    assert!(
        db.daily_timecards(id, date, "name", false)
            .unwrap()
            .rows
            .iter()
            .find(|r| r.card.employee_code == "E002")
            .unwrap()
            .card
            .punches
            .is_empty()
    );
    // Deleting one publication cascades its refs, never bodies used by other history.
    provider
        .exec("DELETE FROM publications WHERE id=?", [s(&first, "id")])
        .unwrap();
    assert_eq!(
        provider
            .count(
                "SELECT count(*) FROM paycom_timecard_payload_refs WHERE publication_id=?",
                [s(&first, "id")]
            )
            .unwrap(),
        0
    );
    assert_eq!(
        provider
            .count("SELECT count(*) FROM paycom_payloads", [])
            .unwrap(),
        3
    );
    assert!(keeper.published(&provider).is_ok());
    // A marker whose ref disappeared is an error on every independent publication reader.
    provider
        .exec(
            "DELETE FROM paycom_timecard_payload_refs
         WHERE publication_id=? AND employee_code='E003' AND date=?",
            [&active.0, date],
        )
        .unwrap();
    assert_eq!(
        db.employee_timecard(id, "E003", None).unwrap_err().code,
        "paycom_payload_unavailable"
    );
    assert_eq!(
        db.daily_timecards(id, date, "name", false)
            .unwrap_err()
            .code,
        "paycom_payload_unavailable"
    );
    assert_eq!(
        keeper.published(&provider).unwrap_err().code,
        "paycom_payload_unavailable"
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

#[tokio::test]
async fn cancelled_live_staging_evicts_a_read_started_during_its_commit()
-> dispatch_core::Result<()> {
    use dispatch_core::{
        State,
        server::cache::{DataDomain, Evicted, Scope},
    };
    use std::sync::mpsc;
    #[derive(Debug)]
    struct Finished(Option<tokio::sync::oneshot::Sender<()>>);
    impl Drop for Finished {
        fn drop(&mut self) {
            // Dropping a completed worker result, after the async caller has left.
            if let Some(done) = self.0.take() {
                let _ = done.send(());
            }
        }
    }
    install();
    let (_root, db, dsp) = bootstrapped();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config)?;
    let read = dispatch_timecard::FEATURE.cached.iter().find(|cached| {
        matches!(&cached.evicted, Evicted::By(domains) if domains.contains(&DataDomain::LIVE))
    }).expect("Timecard declares a cached live read").read;
    let scope = Scope::tenant(read, dsp.clone());
    let at = dsp.clone();
    state
        .run_bookkeeping(move |db| db.dsp(&at)?.set("live-staging-probe", &json!(0)))
        .await?;
    let (began, writing) = tokio::sync::oneshot::channel();
    let (commit, wait_commit) = mpsc::channel();
    let (finished, completion) = tokio::sync::oneshot::channel();
    let writer = tokio::spawn({
        let (state, at) = (state.clone(), dsp.clone());
        async move {
            state
                .run_live_staging(at.clone(), move |db| {
                    let data = db.dsp(&at)?;
                    data.transaction(|| {
                        data.set("live-staging-probe", &json!(1))?;
                        let _ = began.send(());
                        let _ = wait_commit.recv();
                        Ok(())
                    })?;
                    Ok(Finished(Some(finished)))
                })
                .await
        }
    });
    writing.await.unwrap();
    let shared = state.transition.try_read().is_ok();
    if !shared {
        commit.send(()).unwrap();
        drop(writer.await.unwrap()?);
        panic!("live staging blocked foreground readers");
    }
    let (reading, started) = tokio::sync::oneshot::channel();
    let (finish_read, wait_read) = mpsc::channel();
    let reader = tokio::spawn({
        let (state, at, scope) = (state.clone(), dsp.clone(), scope.clone());
        let cache = state.clone();
        async move {
            state
                .read(move |db| {
                    cache.read_cache.read(scope, "day".into(), 1, || {
                        let before = db.dsp(&at)?.setting("live-staging-probe", json!(-1))?;
                        let _ = reading.send(());
                        let _ = wait_read.recv();
                        Ok(before)
                    })
                })
                .await
        }
    });
    started.await.unwrap();
    writer.abort();
    assert!(writer.await.unwrap_err().is_cancelled());
    commit.send(()).unwrap();
    finish_read.send(()).unwrap();
    assert_eq!(reader.await.unwrap()?, json!(0));
    completion.await.unwrap();
    let cache = state.clone();
    let after = state
        .read(move |db| {
            cache.read_cache.read(scope, "day".into(), 1, || {
                db.dsp(&dsp)?.setting("live-staging-probe", json!(-1))
            })
        })
        .await?;
    assert_eq!(
        after,
        json!(1),
        "a cancelled save's worker must drain and evict the late cached read"
    );
    Ok(())
}
