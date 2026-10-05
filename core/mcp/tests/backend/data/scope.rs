use super::*;

fn read(text: &str) -> (String, String) {
    let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(); // a Thursday
    let p = read_period(text, today).unwrap();
    (p.first(), p.last())
}

#[test]
fn periods_read_like_people_write_them() {
    assert_eq!(read("today"), ("2026-10-01".into(), "2026-10-01".into()));
    assert_eq!(
        read("Yesterday"),
        ("2026-09-30".into(), "2026-09-30".into())
    );
    // Amazon's weeks run Sunday to Saturday.
    assert_eq!(
        read("this week"),
        ("2026-09-27".into(), "2026-10-01".into())
    );
    assert_eq!(
        read("last week"),
        ("2026-09-20".into(), "2026-09-26".into())
    );
    assert_eq!(
        read("last month"),
        ("2026-09-01".into(), "2026-09-30".into())
    );
    assert_eq!(
        read("last 7 days"),
        ("2026-09-25".into(), "2026-10-01".into())
    );
    assert_eq!(
        read("2026-09-12"),
        ("2026-09-12".into(), "2026-09-12".into())
    );
    assert_eq!(
        read("2026-09-01..2026-09-15"),
        ("2026-09-01".into(), "2026-09-15".into())
    );
    assert_eq!(read("2026-w39"), ("2026-09-20".into(), "2026-09-26".into()));
    // How people ask about a route day, and longer spans.
    assert_eq!(
        read("last night"),
        ("2026-09-30".into(), "2026-09-30".into())
    );
    assert_eq!(
        read("past week"),
        ("2026-09-25".into(), "2026-10-01".into())
    );
    assert_eq!(
        read("last 2 weeks"),
        ("2026-09-18".into(), "2026-10-01".into())
    );
    assert_eq!(
        read("last 30 days"),
        ("2026-09-02".into(), "2026-10-01".into())
    );
    // A count too large to be days is no period, not an overflow.
    let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    assert!(read_period("last 1317624576693539402 weeks", today).is_none());
    let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    for nonsense in [
        "soon",
        "last 0 days",
        "2026-09-15..2026-09-01",
        "2026-13-01",
    ] {
        assert!(read_period(nonsense, today).is_none(), "{nonsense}");
    }
}
#[test]
fn a_period_is_one_form_and_at_most_a_year() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
    let q = |v: Value| period(&v, today, "today");
    assert_eq!(q(serde_json::json!({})).unwrap().first(), "2026-10-01");
    assert_eq!(
        q(serde_json::json!({"from":"2026-09-01","to":"2026-09-02"}))
            .unwrap()
            .last(),
        "2026-09-02"
    );
    assert_eq!(
        q(serde_json::json!({"date":"today","period":"last week"}))
            .unwrap_err()
            .code,
        "period_conflict"
    );
    assert_eq!(
        q(serde_json::json!({"from":"2025-01-01","to":"2026-09-01"}))
            .unwrap_err()
            .code,
        "period_too_long"
    );
    assert_eq!(
        q(serde_json::json!({"from":"2026-09-01"}))
            .unwrap_err()
            .code,
        "invalid_period"
    );
}
#[test]
fn drivers_are_found_by_code_id_or_name_and_never_guessed() {
    let person = |code: &str, name: &str, paycom: &str, amazon: &str| Person {
        code: code.into(),
        name: name.into(),
        status: DriverStatus::Matched,
        paycom: vec![paycom.into()],
        amazon: vec![amazon.into()],
    };
    let list = vec![
        person("K4M7QZ", "Daniel Ortiz", "E002", "A1B2C3"),
        person("T9X2PB", "Daniel Reyes", "E003", "D4E5F6"),
        person("H7N3QA", "Avery Morgan", "E004", "G7H8J9"),
    ];
    let people = People {
        holders: HashMap::new(),
        list,
        bypassed: vec![],
    };
    assert_eq!(people.find("k4m7qz").unwrap().name, "Daniel Ortiz");
    assert_eq!(people.find("E003").unwrap().code, "T9X2PB");
    assert_eq!(people.find("G7H8J9").unwrap().code, "H7N3QA");
    assert_eq!(people.find("daniel ortiz").unwrap().code, "K4M7QZ");
    assert_eq!(people.find("avery").unwrap().code, "H7N3QA");
    assert_eq!(people.find("Dan Rey").unwrap().code, "T9X2PB");
    let both = people.find("Daniel").unwrap_err();
    assert_eq!(both.code, "driver_ambiguous");
    assert_eq!(both.choices.len(), 2);
    assert_eq!(people.find("Quinn").unwrap_err().code, "driver_not_found");
}
