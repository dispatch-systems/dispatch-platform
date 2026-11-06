use super::*;
use crate::timecards::PeriodSync;
fn body() -> Value {
    let mut body = json!({});
    for key in FIELDS {
        body[*key] = Value::Null;
    }
    body["eeCodes"] = json!(["AA01", "BB02"]);
    body["startDate"] = json!("2026-08-30");
    body["endDate"] = json!("2026-09-12");
    body["isAdvancedFilterApplied"] = json!(true);
    body["onlyBorrowedEmployees"] = json!(false);
    body
}
#[test]
fn rejects_partial_rosters_and_selects_timezone_date_period() -> Result<()> {
    let (selected, period, _) = selected_body(&body(), date("2026-09-16")?)?;
    assert_eq!(period["start"], "2026-09-13");
    assert_eq!(period["end"], "2026-09-26");
    assert_eq!(selected["isAdvancedFilterApplied"], false);
    let (_, historical, _) = selected_body(&body(), date("2026-01-10")?)?;
    assert_eq!(historical["start"], "2026-01-04");
    assert_eq!(historical["end"], "2026-01-17");
    for (key, value) in [
        ("q", json!("driver")),
        ("take", json!(1)),
        ("skip", json!(1)),
        ("onlyBorrowedEmployees", json!(true)),
        ("isAdvancedFilterApplied", Value::Null),
        ("eeCodes", json!(["AA01", "aa01"])),
    ] {
        let mut body = body();
        body[key] = value;
        assert!(selected_body(&body, date("2026-09-16")?).is_err());
    }
    assert!(codes(&json!(["AA01", "aa01"])).is_err());
    Ok(())
}
#[test]
fn requested_days_keep_the_cycle_observed_in_paycom() -> Result<()> {
    let mut observed = body();
    observed["startDate"] = json!("2026-09-06");
    observed["endDate"] = json!("2026-09-19");
    for (day, from, to) in [
        ("2026-09-05", "2026-08-23", "2026-09-05"),
        ("2026-09-06", "2026-09-06", "2026-09-19"),
        ("2026-09-19", "2026-09-06", "2026-09-19"),
        ("2026-09-20", "2026-09-20", "2026-10-03"),
        ("2025-12-31", "2025-12-28", "2026-01-10"),
        ("2026-03-08", "2026-03-08", "2026-03-21"),
        ("2028-02-29", "2028-02-20", "2028-03-04"),
    ] {
        let (selected, period, _) = selected_body(&observed, date(day)?)?;
        assert_eq!(selected["startDate"], from);
        assert_eq!(selected["endDate"], to);
        assert_eq!(period["dates"].as_array().unwrap().len(), 14);
        assert_eq!(period["dates"][0], from);
        assert_eq!(period["dates"][13], to);
    }
    Ok(())
}

// Scope and roster union belong here; native coverage proves these employees actually
// reach their authorized pages. Ordinary active-roster validation remains unchanged.
#[test]
fn full_period_requests_include_known_unlisted_employees_and_require_every_day() -> Result<()> {
    let request = json!({"allEmployees":true,"from":"2026-09-06","to":"2026-09-19"});
    let scope = PeriodSync::parse(&request)?.unwrap();
    assert!(PeriodSync::parse(&json!({}))?.is_none());
    for change in [
        json!({"allEmployees":false,"from":"2026-09-06","to":"2026-09-19"}),
        json!({"allEmployees":true,"from":"2026-09-06","to":"2026-09-20"}),
        json!({"allEmployees":true,"from":"2026-09-06","to":"2026-09-19","employeeCode":"AA01"}),
        json!({"allEmployees":true,"from":"2026-09-06","to":"2026-09-19","date":"2026-09-06"}),
    ] {
        assert!(PeriodSync::parse(&change).is_err());
    }
    let future = PeriodSync {
        all_employees: true,
        from: "2999-01-06".into(),
        to: "2999-01-19".into(),
    };
    assert!(future.date("UTC").is_err());
    let capture = scope.fixture("UTC")?;
    scope.validate_capture(&capture)?;
    let mut incomplete = capture.clone();
    incomplete["timecards"].as_array_mut().unwrap().pop();
    assert_eq!(
        scope.validate_capture(&incomplete).unwrap_err().code,
        "timecard_identity_mismatch"
    );
    let different = PeriodSync {
        all_employees: true,
        from: "2026-08-23".into(),
        to: "2026-09-05".into(),
    };
    assert_eq!(
        different.validate_capture(&capture).unwrap_err().code,
        "timecard_identity_mismatch"
    );
    let active = json!({"code":"AA01","name":"Current name","department":"Driver","position":"Driver","station":"S","active":true});
    let mut older = active.clone();
    older["name"] = json!("Previous name");
    let mut missing = active.clone();
    missing["code"] = json!("BB02");
    let employees = with_known_employees(
        vec![active.clone()],
        &[older, missing],
        &scope.period().provider_period()?,
    )?;
    assert_eq!(employees.len(), 2);
    assert_eq!(employees[0], active);
    assert_eq!(employees[1]["code"], "BB02");
    assert_eq!(employees[1]["active"], false);
    Ok(())
}
