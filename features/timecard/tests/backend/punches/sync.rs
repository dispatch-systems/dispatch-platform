use super::*;
use crate::backend::TimecardStore;
use dispatch_core::{
    foundation::config::{Config, Environment},
    server::{operations, startup},
};
use dispatch_paycom::fixtures;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[test]
fn a_scoped_publication_rejects_other_employees_periods_and_incomplete_captures() -> Result<()> {
    crate::testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&crate::FEATURE],
    );
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut config = Config::load()?;
    config.root = root.path().into();
    config.development = true;
    config.fixture = true;
    config.environment = Environment::Preview;
    let store = startup::prepare_storage(config)?;
    let bootstrap = operations::bootstrap(
        &store,
        "sync@example.test",
        "Sync",
        "Owner",
        "test-password-long",
    )?;
    let id = s(&bootstrap["dsp"], "id");
    let full = fixtures::fixture_date("UTC", Some("2026-08-22".parse().unwrap()))?;
    store.publish_timecards(id, &full)?;
    let scope = EmployeeSync {
        employee_code: "E001".into(),
        from: "2026-08-09".into(),
        to: "2026-08-22".into(),
    };
    let mut capture = scope.fixture("UTC")?;
    capture["collectedAt"] = json!("2099-01-02T00:00:00Z");
    capture["sources"] = json!([{"employeeCode":"E001","periodKey":"P1",
        "url":"https://paycom.example/timecard/accepted"}]);
    publish_employee_timecard(&store, id, &scope, &capture)?;
    let saved = store.employee_timecard(id, "E001", Some(&scope.period()))?;
    assert_eq!(saved.timecards.len(), 14);
    let mut wrong = capture.clone();
    wrong["employees"] = full["employees"].clone();
    assert!(publish_employee_timecard(&store, id, &scope, &wrong).is_err());
    let mut wrong = capture.clone();
    wrong["from"] = json!("2026-08-08");
    assert!(publish_employee_timecard(&store, id, &scope, &wrong).is_err());
    let mut wrong = capture.clone();
    wrong["timecards"].as_array_mut().unwrap().pop();
    assert!(publish_employee_timecard(&store, id, &scope, &wrong).is_err());
    let other = EmployeeSync {
        employee_code: "E002".into(),
        ..scope.clone()
    };
    assert!(publish_employee_timecard(&store, id, &other, &capture).is_err());
    assert_eq!(
        store.employee_timecard(id, "E001", Some(&scope.period()))?,
        saved
    );
    assert_eq!(
        super::super::employees::roster(&store, id)?
            .page("", 0, None, false, None)
            .total,
        12
    );
    let db = store.collector(id, paycom::PROVIDER)?;
    let pool = db.all("SELECT * FROM paycom_payloads ORDER BY format,digest", [])?;
    let refs = db.all("SELECT * FROM paycom_employee_sync_payload_refs", [])?;
    assert!(
        pool.is_empty(),
        "employee sync must not write pooled bodies"
    );
    assert!(refs.is_empty(), "employee sync must not write references");
    for expected in capture["timecards"].as_array().unwrap() {
        let actual = saved
            .timecards
            .iter()
            .find(|card| card.card.date == expected["date"])
            .unwrap();
        let punches: Vec<crate::Punch> = serde_json::from_value(expected["punches"].clone())?;
        assert_eq!(actual.card.punches, punches);
    }
    let mut stale = capture.clone();
    stale["collectedAt"] = json!("2099-01-01T00:00:00Z");
    stale["timecards"][13]["punches"][0]["in"] = json!("07:55");
    stale["sources"][0]["url"] = json!("https://paycom.example/timecard/stale");
    publish_employee_timecard(&store, id, &scope, &stale)?;
    assert_eq!(
        store.employee_timecard(id, "E001", Some(&scope.period()))?,
        saved
    );
    assert_eq!(
        db.all("SELECT * FROM paycom_payloads ORDER BY format,digest", [])?,
        pool
    );
    assert_eq!(
        db.all("SELECT * FROM paycom_employee_sync_payload_refs", [])?,
        refs
    );

    // Equal timestamps still replace; a later envelope keeps all JSON inline.
    capture["timecards"][13]["punches"][0]["in"] = json!("07:56");
    publish_employee_timecard(&store, id, &scope, &capture)?;
    assert_eq!(db.count("SELECT count(*) FROM paycom_payloads", [])?, 0);
    capture["collectedAt"] = json!("2099-01-03T00:00:00Z");
    publish_employee_timecard(&store, id, &scope, &capture)?;
    assert_eq!(db.count("SELECT count(*) FROM paycom_payloads", [])?, 0);
    assert_eq!(
        db.one("SELECT data FROM employee_timecard_syncs", [])?
            .unwrap()["data"],
        capture.to_string()
    );
    // A stored-format fixture with a scalar timestamp distinct from the raw envelope.
    let mut body = capture.clone();
    body.as_object_mut().unwrap().remove("collectedAt");
    // Old-build stored-format fixture: recursively sorted canonical JSON, not a product writer.
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(fields) => {
                let fields: std::collections::BTreeMap<_, _> = fields
                    .iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect();
                serde_json::to_value(fields).unwrap()
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    let body = sorted(&body).to_string();
    let digest =
        dispatch_core::foundation::crypto::sha(format!("paycom.employee-capture.v1\0{body}"));
    let attach = || -> Result<()> {
        db.exec("INSERT INTO paycom_employee_sync_payload_refs VALUES (?,?,?,'paycom.employee-capture.v1',?,?)",
            params!["E001", scope.from, scope.to, digest, s(&capture, "collectedAt")])?;
        Ok(())
    };
    db.transaction(|| {
        db.exec(
            "INSERT INTO paycom_payloads VALUES ('paycom.employee-capture.v1',?,?)",
            params![digest, body],
        )?;
        db.exec(
            r#"UPDATE employee_timecard_syncs SET collected_at='2099-01-04T00:00:00Z',
               data='{"$dispatchPayload":"paycom.employee-capture.v1"}'"#,
            [],
        )?;
        attach()
    })?;
    let joined = db
        .one(
            "SELECT s.data,s.collected_at,r.format payload_format,b.body payload_body,
         r.payload_collected_at FROM employee_timecard_syncs s
         LEFT JOIN paycom_employee_sync_payload_refs r ON r.employee_code=s.employee_code
         AND r.period_from=s.period_from AND r.period_to=s.period_to
         LEFT JOIN paycom_payloads b ON b.format=r.format AND b.digest=r.digest",
            [],
        )?
        .unwrap();
    assert_eq!(
        super::super::payloads::capture(&joined)?["collectedAt"],
        "2099-01-03T00:00:00Z"
    );
    let compact = store.employee_timecard(id, "E001", Some(&scope.period()))?;
    assert_eq!(
        compact.collected_at.as_deref(),
        Some("2099-01-04T00:00:00Z")
    );
    assert_eq!(
        compact.timecards[0].card.punches[0].clock_in.as_deref(),
        Some("07:56")
    );
    assert_eq!(
        compact.timecards[0].card.source_url.as_deref(),
        Some("https://paycom.example/timecard/accepted")
    );
    let selected = ["E001".to_owned()];
    let range =
        store.daily_timecards_range(id, &scope.from, &scope.to, "name", false, Some(&selected))?;
    assert_eq!(
        range[&scope.to].rows[0].card.punches[0].clock_in.as_deref(),
        Some("07:56")
    );
    assert_eq!(
        range[&scope.to].rows[0].card.source_url.as_deref(),
        Some("https://paycom.example/timecard/accepted")
    );

    // Missing ref/body cannot turn a compact employee capture into empty payroll.
    db.transaction(|| {
        db.exec("DELETE FROM paycom_employee_sync_payload_refs", [])?;
        assert_eq!(
            store
                .employee_timecard(id, "E001", Some(&scope.period()))
                .unwrap_err()
                .code,
            "paycom_payload_unavailable"
        );
        assert_eq!(
            store
                .daily_timecards(id, &scope.to, "name", false)
                .unwrap_err()
                .code,
            "paycom_payload_unavailable"
        );
        attach()
    })?;

    // Rollback REPLACE/UPDATE and DELETE invalidate only that capture, with full inline fallback.
    capture["sources"][0]["url"] = json!("https://paycom.example/timecard/legacy");
    db.exec(
        "INSERT OR REPLACE INTO employee_timecard_syncs VALUES (?,?,?,?,?)",
        params![
            "E001",
            scope.from,
            scope.to,
            "2099-01-05T00:00:00Z",
            capture.to_string()
        ],
    )?;
    assert_eq!(
        db.count("SELECT count(*) FROM paycom_employee_sync_payload_refs", [])?,
        0
    );
    assert_eq!(
        store
            .employee_timecard(id, "E001", Some(&scope.period()))?
            .timecards[0]
            .card
            .source_url
            .as_deref(),
        Some("https://paycom.example/timecard/legacy")
    );
    capture["collectedAt"] = json!("2099-01-06T00:00:00Z");
    publish_employee_timecard(&store, id, &scope, &capture)?;
    // Seed an existing old-build reference before exercising invalidation.
    db.exec("INSERT INTO paycom_employee_sync_payload_refs VALUES (?,?,?,'paycom.employee-capture.v1',?,?)",
        params!["E001", scope.from, scope.to, digest, s(&capture, "collectedAt")])?;
    db.exec(
        "UPDATE employee_timecard_syncs SET collected_at='2099-01-06T01:00:00Z'",
        [],
    )?;
    assert_eq!(
        db.count("SELECT count(*) FROM paycom_employee_sync_payload_refs", [])?,
        0
    );
    assert!(
        store
            .employee_timecard(id, "E001", Some(&scope.period()))
            .is_ok()
    );
    capture["collectedAt"] = json!("2099-01-07T00:00:00Z");
    publish_employee_timecard(&store, id, &scope, &capture)?;

    // An old-build reference makes both rollback and deletion invalidation non-vacuous.
    let mut current_body = capture.clone();
    current_body.as_object_mut().unwrap().remove("collectedAt");
    let current_body = sorted(&current_body).to_string();
    let current_digest = dispatch_core::foundation::crypto::sha(format!(
        "paycom.employee-capture.v1\0{current_body}"
    ));
    db.exec(
        "INSERT INTO paycom_payloads VALUES ('paycom.employee-capture.v1',?,?)",
        params![current_digest, current_body],
    )?;
    db.exec("INSERT INTO paycom_employee_sync_payload_refs VALUES (?,?,?,'paycom.employee-capture.v1',?,?)",
        params!["E001", scope.from, scope.to, current_digest, s(&capture, "collectedAt")])?;
    let before_rows = db.all("SELECT * FROM employee_timecard_syncs", [])?;
    let before_refs = db.all("SELECT * FROM paycom_employee_sync_payload_refs", [])?;
    assert_eq!(before_refs.len(), 1);
    let before_pool = db.all("SELECT * FROM paycom_payloads ORDER BY format,digest", [])?;
    db.0.execute_batch(
        "CREATE TRIGGER reject_sync_write AFTER UPDATE ON employee_timecard_syncs
         BEGIN SELECT RAISE(ABORT,'fixture_sync_failure'); END;",
    )?;
    capture["collectedAt"] = json!("2099-01-08T00:00:00Z");
    capture["timecards"][13]["punches"][0]["in"] = json!("07:54");
    assert!(publish_employee_timecard(&store, id, &scope, &capture).is_err());
    assert_eq!(
        db.all("SELECT * FROM employee_timecard_syncs", [])?,
        before_rows
    );
    assert_eq!(
        db.all("SELECT * FROM paycom_employee_sync_payload_refs", [])?,
        before_refs
    );
    assert_eq!(
        db.all("SELECT * FROM paycom_payloads ORDER BY format,digest", [])?,
        before_pool
    );
    db.0.execute_batch("DROP TRIGGER reject_sync_write;")?;
    db.exec("DELETE FROM employee_timecard_syncs", [])?;
    assert_eq!(
        db.count("SELECT count(*) FROM paycom_employee_sync_payload_refs", [])?,
        0
    );
    assert_eq!(
        store
            .employee_timecard(id, "E001", Some(&scope.period()))?
            .employee
            .name,
        "Avery Morgan"
    );
    assert!(
        EmployeeSync::parse(
            &json!({"employeeCode":"E001","from":scope.from,"to":scope.to,"date":scope.from})
        )
        .is_err()
    );
    Ok(())
}
