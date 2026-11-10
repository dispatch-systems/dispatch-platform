use super::*;
use serde_json::json;

#[test]
fn an_answer_is_read_as_a_list_a_list_under_data_or_one_record_and_refused_when_it_failed() {
    let record = json!({"position": "Driver"});
    for (answer, expected) in [
        (json!([record.clone()]), vec![record.clone()]),
        (
            json!({"result": true, "data": [record.clone()]}),
            vec![record.clone()],
        ),
        (
            json!({"result": true, "data": record.clone()}),
            vec![record.clone()],
        ),
        (json!({"result": true, "data": null}), vec![]),
        (record.clone(), vec![record.clone()]),
    ] {
        assert_eq!(rows(&answer.to_string()).unwrap(), expected, "{answer}");
    }
    for answer in [
        json!({"result": false, "errors": ["no"]}).to_string(),
        json!({"data": "text"}).to_string(),
        "\"text\"".to_owned(),
        "<html>".to_owned(),
    ] {
        assert!(
            rows(&answer).unwrap_err().is(codes::API_UNREADABLE),
            "{answer}"
        );
    }
}

#[test]
fn api_credentials_are_an_sid_a_token_and_at_most_a_sunday_to_count_pay_periods_from() {
    let parsed =
        Credentials::parse(&json!({"sid":"abc123","token":"t0ken","periodStart":"2026-11-01"}))
            .unwrap();
    assert_eq!(
        (
            parsed.sid.as_str(),
            parsed.token.as_str(),
            parsed.period_start
        ),
        ("abc123", "t0ken", NaiveDate::from_ymd_opt(2026, 11, 1))
    );
    assert!(Credentials::parse(&json!({"sid":"abc123","token":"t0ken"})).is_ok());
    for refused in [
        // A Monday, a header's line break, a field of the sign-in's, and nothing at all.
        json!({"sid":"abc123","token":"t0ken","periodStart":"2026-11-02"}),
        json!({"sid":"abc\r\nX: y","token":"t0ken"}),
        json!({"sid":"abc123","token":"t0ken","password":"p"}),
        json!({"sid":"","token":"t0ken"}),
    ] {
        assert!(Credentials::parse(&refused).is_err(), "{refused}");
    }
}

#[test]
fn a_connection_is_ready_only_when_every_check_passed_and_fails_by_its_first_failure() {
    let mut checks = Checks::waiting();
    for id in CHECKS {
        checks.pass(id);
    }
    assert!(checks.outcome().is_ok());
    // Nobody on the roster leaves the employee functions unasked: not ready, and no failure.
    let mut unasked = Checks::waiting();
    for id in [CREDENTIALS, EMPLOYEE_DIRECTORY, CATEGORIES, PAY_PERIOD] {
        unasked.pass(id);
    }
    assert_eq!(unasked.outcome().unwrap_err().code, "roster_not_complete");
    checks.answered(PUNCH_HISTORY, &Answered::Off);
    checks.fail(PAY_PERIOD, codes::PERIOD_REQUIRED);
    assert!(checks.outcome().unwrap_err().is(codes::API_FUNCTION_OFF));
    let failed = checks
        .list()
        .into_iter()
        .filter(|check| check.state == CheckState::Failed)
        .map(|check| (check.id, check.error.unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        failed,
        [
            (
                PUNCH_HISTORY.to_owned(),
                "paycom_api_function_off".to_owned()
            ),
            (PAY_PERIOD.to_owned(), "paycom_period_required".to_owned()),
        ]
    );
}

#[test]
fn production_refuses_api_credentials_before_a_connection_is_touched_and_takes_a_sign_ins() {
    dispatch_core::testing::install(&[&crate::COLLECTOR], &[]);
    let (_root, mut store, dsp) = dispatch_core::testing::bootstrapped();
    let api = json!({"sid":"abc123","token":"t0ken"});
    let sign_in = json!({"clientCode":"TEST","username":"user","password":"synthetic-password",
        "securityAnswers":["one","two","three","four","five"]});
    for credentials in [&api, &sign_in] {
        assert!(
            store
                .accepts_credentials(&dsp, crate::PROVIDER, credentials)
                .is_ok()
        );
    }
    store.config.environment = dispatch_core::foundation::config::Environment::Production;
    let refused = store
        .accepts_credentials(&dsp, crate::PROVIDER, &api)
        .unwrap_err();
    assert!(refused.is(codes::API_UNAVAILABLE));
    assert!(
        store
            .accepts_credentials(&dsp, crate::PROVIDER, &sign_in)
            .is_ok()
    );
}
