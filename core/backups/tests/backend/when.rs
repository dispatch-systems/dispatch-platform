//! When a backup rule runs: on its days of the week, or its dates of the month, a date a month
//! doesn't have on that month's last day; at its times, where the DSP is.
use super::*;
use serde_json::json;

fn rule(value: Value) -> When {
    When::parse(&value).unwrap()
}
fn at(text: &str, tz: &TimeZone) -> i64 {
    let local = jiff::civil::DateTime::strptime("%Y-%m-%d %H:%M", text).unwrap();
    crate::foundation::clock::only_instant(tz, local).unwrap()
}
/// Its next `count` runs after `from`, where the DSP is.
fn runs(when: &When, tz: &TimeZone, from: &str, count: usize) -> Vec<String> {
    upcoming(std::slice::from_ref(when), tz, at(from, tz), count)
        .into_iter()
        .map(|run| {
            let local = crate::foundation::clock::local(tz, run);
            local.strftime("%Y-%m-%d %H:%M").to_string()
        })
        .collect()
}

#[test]
fn a_rule_runs_on_its_dates_a_date_past_the_month_on_its_last_day() {
    let tz = crate::foundation::clock::zone("America/Chicago").unwrap();
    let month = |dates: Value, last: bool| {
        rule(
            json!({"on": "month", "dates": dates, "lastDay": last, "kind": "times",
            "times": ["02:00"]}),
        )
    };
    // The 1st and 15th, a few times a month.
    assert_eq!(
        runs(&month(json!([1, 15]), false), &tz, "2027-01-20 12:00", 3),
        ["2027-02-01 02:00", "2027-02-15 02:00", "2027-03-01 02:00"]
    );
    // The 31st runs on the last day of a month without one; the 30th once on February's last.
    assert_eq!(
        runs(&month(json!([31]), false), &tz, "2027-01-01 00:00", 4),
        [
            "2027-01-31 02:00",
            "2027-02-28 02:00",
            "2027-03-31 02:00",
            "2027-04-30 02:00"
        ]
    );
    assert_eq!(
        runs(&month(json!([30]), false), &tz, "2028-02-01 00:00", 2),
        ["2028-02-29 02:00", "2028-03-30 02:00"]
    );
    // The last day, once a month, and a date beside it runs once on a day both name.
    assert_eq!(
        runs(&month(json!([]), true), &tz, "2027-02-01 00:00", 2),
        ["2027-02-28 02:00", "2027-03-31 02:00"]
    );
    assert_eq!(
        runs(&month(json!([28]), true), &tz, "2027-02-01 00:00", 3),
        ["2027-02-28 02:00", "2027-03-28 02:00", "2027-03-31 02:00"]
    );
    // Days of the week, every few hours between two times.
    let weekdays = rule(
        json!({"on": "week", "days": [0, 4], "kind": "repeat", "every": 240,
        "from": "06:00", "to": "14:00"}),
    );
    assert_eq!(
        runs(&weekdays, &tz, "2027-01-04 07:00", 4),
        [
            "2027-01-04 10:00",
            "2027-01-04 14:00",
            "2027-01-08 06:00",
            "2027-01-08 10:00"
        ]
    );
    // A month's rule names a date or its last day; a week's, a day.
    for wrong in [
        json!({"on": "month", "dates": [], "lastDay": false, "kind": "times", "times": ["02:00"]}),
        json!({"on": "month", "dates": [32], "lastDay": false, "kind": "times", "times": ["02:00"]}),
        json!({"on": "week", "days": [], "kind": "times", "times": ["02:00"]}),
    ] {
        assert_eq!(
            When::parse(&wrong).unwrap_err().code,
            "invalid_schedule_days"
        );
    }
}

#[test]
fn the_calendar_names_the_rules_that_run_each_day() {
    let first = rule(
        json!({"on": "month", "dates": [1], "lastDay": false, "kind": "times",
        "times": ["02:00"]}),
    );
    let weekends = rule(json!({"on": "week", "days": [5, 6], "kind": "times",
        "times": ["03:00"]}));
    let start = jiff::civil::date(2027, 1, 29);
    let days: Vec<(String, Vec<usize>)> = calendar(&[first, weekends], start, 4)
        .into_iter()
        .map(|(day, rules)| (day.strftime("%m-%d").to_string(), rules))
        .collect();
    assert_eq!(
        days,
        [
            ("01-29".to_owned(), vec![]),
            ("01-30".to_owned(), vec![1]),
            ("01-31".to_owned(), vec![1]),
            ("02-01".to_owned(), vec![0]),
        ]
    );
}
