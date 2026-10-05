use super::*;
fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}
#[test]
fn weeks_run_sunday_to_saturday_and_are_named_after_the_saturday() {
    assert_eq!(
        week_days("2026-W38").unwrap(),
        (date("2026-09-13"), date("2026-09-19"))
    );
    assert_eq!(week_of(date("2026-09-19")), "2026-W38");
    // Every day of the following week still sees week 38 as the last completed one.
    for day in ["2026-09-20", "2026-09-23", "2026-09-26"] {
        assert_eq!(last_completed_week(date(day)), "2026-W38", "{day}");
    }
    assert_eq!(last_completed_week(date("2026-09-27")), "2026-W39");
    assert_eq!(
        weeks_before("2026-W02", 3).unwrap(),
        ["2026-W02", "2026-W01", "2025-W52", "2025-W51"]
    );
    for invalid in ["2026-W00", "2026-W54", "2026-38", "26-W38", "2026-W3"] {
        assert!(parse_week(invalid).is_err(), "{invalid}");
    }
    assert!(week_or_latest(Some("2026-W39"), date("2026-09-25")).is_err());
    assert_eq!(
        week_or_latest(None, date("2026-09-25")).unwrap(),
        "2026-W38"
    );
}
