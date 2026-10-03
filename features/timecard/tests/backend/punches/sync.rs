use super::*;
use crate::backend::TimecardStore;
use dispatch_core::{foundation::config::Config, server::operations};
use dispatch_paycom::fixtures;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[test]
fn a_scoped_publication_rejects_other_employees_periods_and_incomplete_captures() -> Result<()> {
    crate::testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&dispatch_driver_match::FEATURE, &crate::FEATURE],
    );
    let root = tempfile::tempdir()?;
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut config = Config::load()?;
    config.root = root.path().into();
    config.development = true;
    config.fixture = true;
    config.environment = "preview".into();
    let store = Store::initialize(config)?;
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
    let capture = scope.fixture("UTC")?;
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
        store
            .timecard_employees(id, "", 0, None, false, None)
            .map(|value| serde_json::to_value(value).unwrap())?["total"],
        12
    );
    assert!(
        EmployeeSync::parse(
            &json!({"employeeCode":"E001","from":scope.from,"to":scope.to,"date":scope.from})
        )
        .is_err()
    );
    Ok(())
}
