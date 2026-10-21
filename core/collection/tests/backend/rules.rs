use super::*;
use serde_json::json;

fn ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}
fn at(value: i64) -> String {
    crate::db::at(value)
}
fn rule(value: Value) -> Rule {
    Rule::parse(&value).unwrap()
}

#[test]
fn a_rule_runs_on_its_days_at_its_local_times_across_clock_changes() {
    let chicago: Tz = "America/Chicago".parse().unwrap();
    let new_york: Tz = "America/New_York".parse().unwrap();
    let weekdays = rule(json!({"kind":"times","days":[0,1,2,3,4],"times":["14:00","06:00"]}));
    let repeat = rule(json!({"kind":"repeat","days":[5],"every":120,"from":"22:00","to":"02:00"}));
    let all_day = rule(json!({"kind":"repeat","days":[0,1,2,3,4,5,6],"every":720,
        "from":"06:00","to":"06:00"}));
    let night = rule(json!({"kind":"times","days":[6],"times":["02:30","01:30"]}));
    let cases = [
        // Friday 2:00 PM has passed: Monday 6:00 AM.
        (
            &weekdays,
            chicago,
            "2026-01-09T20:30:00Z",
            "2026-01-12T12:00:00.000Z",
        ),
        (
            &weekdays,
            chicago,
            "2026-01-12T12:00:00Z",
            "2026-01-12T20:00:00.000Z",
        ),
        // Saturday's 10:00 PM to 2:00 AM carries past midnight into Sunday, 2:00 AM included.
        (
            &repeat,
            chicago,
            "2026-01-11T05:00:00Z",
            "2026-01-11T06:00:00.000Z",
        ),
        (
            &repeat,
            chicago,
            "2026-01-11T07:00:00Z",
            "2026-01-11T08:00:00.000Z",
        ),
        (
            &repeat,
            chicago,
            "2026-01-11T08:00:00Z",
            "2026-01-18T04:00:00.000Z",
        ),
        // All day from 6:00 AM: 6:00 PM, then 6:00 AM again, never 6:00 AM twice.
        (
            &all_day,
            chicago,
            "2026-01-10T12:00:00Z",
            "2026-01-11T00:00:00.000Z",
        ),
        (
            &all_day,
            chicago,
            "2026-01-11T00:00:00Z",
            "2026-01-11T12:00:00.000Z",
        ),
        // The spring change skips 2:30 AM, so 1:30 AM runs, then next Sunday's 1:30 AM.
        (
            &night,
            new_york,
            "2026-03-08T05:00:00Z",
            "2026-03-08T06:30:00.000Z",
        ),
        (
            &night,
            new_york,
            "2026-03-08T06:30:00Z",
            "2026-03-15T05:30:00.000Z",
        ),
        // The fall change repeats 1:30 AM, which runs once, at its first.
        (
            &night,
            new_york,
            "2026-11-01T05:00:00Z",
            "2026-11-01T05:30:00.000Z",
        ),
        (
            &night,
            new_york,
            "2026-11-01T05:30:00Z",
            "2026-11-01T07:30:00.000Z",
        ),
    ];
    for (rule, tz, after, expected) in cases {
        let found = rule.next(tz, ms(after)).map(at);
        assert_eq!(
            found.as_deref(),
            Some(expected),
            "{} after {after}",
            rule.describe()
        );
    }
    let both = [weekdays.clone(), weekdays];
    let runs: Vec<String> = upcoming(&both, chicago, ms("2026-01-12T11:00:00Z"), 3)
        .into_iter()
        .map(at)
        .collect();
    assert_eq!(
        runs,
        [
            "2026-01-12T12:00:00.000Z",
            "2026-01-12T20:00:00.000Z",
            "2026-01-13T12:00:00.000Z"
        ]
    );
}

#[test]
fn a_rule_is_refused_unless_its_days_and_times_hold() {
    let refused = [
        (
            json!({"kind":"times","days":[],"times":["06:00"]}),
            "invalid_schedule_days",
        ),
        (
            json!({"kind":"times","days":[7],"times":["06:00"]}),
            "invalid_schedule_days",
        ),
        (
            json!({"kind":"times","days":[1,1],"times":["06:00"]}),
            "invalid_schedule_days",
        ),
        (
            json!({"kind":"times","days":[1],"times":[]}),
            "invalid_schedule_time",
        ),
        (
            json!({"kind":"times","days":[1],"times":["06:00","06:00"]}),
            "invalid_schedule_time",
        ),
        (
            json!({"kind":"times","days":[1],"times":["6:00"]}),
            "invalid_schedule_time",
        ),
        (
            json!({"kind":"times","days":[1],"times":["24:00"]}),
            "invalid_schedule_time",
        ),
        (
            json!({"kind":"times","days":[1],"times":vec!["06:00"; MAX_TIMES + 1]}),
            "invalid_schedule_time",
        ),
        (
            json!({"kind":"repeat","days":[1],"every":15,"from":"06:00","to":"09:00"}),
            "invalid_schedule_interval",
        ),
        (
            json!({"kind":"repeat","days":[1],"every":45,"from":"06:00","to":"09:00"}),
            "invalid_schedule_interval",
        ),
        (
            json!({"kind":"repeat","days":[1],"every":750,"from":"06:00","to":"09:00"}),
            "invalid_schedule_interval",
        ),
        (
            json!({"kind":"repeat","days":[1],"every":30,"from":"06:00"}),
            "invalid_input",
        ),
        (
            json!({"kind":"daily","days":[1],"times":["06:00"]}),
            "invalid_input",
        ),
    ];
    for (value, code) in refused {
        assert_eq!(Rule::parse(&value).unwrap_err().code, code, "{value}");
    }
}

/// The words the activity log keeps of a rule, which the page's own match (the same table,
/// `tests/frontend/rules.test.ts`).
#[test]
fn a_rule_reads_in_the_activity_log_as_the_page_words_it() {
    let table: Vec<(Value, String)> =
        serde_json::from_str(include_str!("../support/rule-words.json")).unwrap();
    for (value, expected) in table {
        assert_eq!(rule(value).describe(), expected);
    }
}
