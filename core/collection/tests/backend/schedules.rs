use super::*;
fn ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}
#[test]
fn daily_time_respects_timezone_and_runs_once_across_dst() {
    assert_eq!(
        next_daily("06:00", "America/Chicago", ms("2026-01-10T13:00:00Z")).unwrap(),
        "2026-01-11T12:00:00.000Z"
    );
    assert_eq!(
        next_daily("02:30", "America/New_York", ms("2026-03-08T05:00:00Z")).unwrap(),
        "2026-03-09T06:30:00.000Z"
    );
    // From the first 01:30 itself, the repeated one is not run again.
    assert_eq!(
        next_daily("01:30", "America/New_York", ms("2026-11-01T05:30:00Z")).unwrap(),
        "2026-11-02T06:30:00.000Z"
    );
    assert!(next_daily("24:00", "UTC", 0).is_err());
    assert!(next_daily("1:00", "UTC", 0).is_err());
}
#[test]
fn intervals_keep_their_anchor_after_delays_and_restarts() {
    let row = Timing {
        cadence: Cadence::Interval,
        interval_minutes: Some(120),
        local_time: "00:00",
        anchor: ms("2026-01-10T08:00:00Z"),
    };
    assert_eq!(
        next(&row, "UTC", ms("2026-01-10T10:17:49Z")).unwrap(),
        "2026-01-10T12:00:00.000Z"
    );
    assert_eq!(
        next(&row, "UTC", ms("2026-01-15T11:59:00Z")).unwrap(),
        "2026-01-15T12:00:00.000Z"
    );
    assert_eq!(
        next(&row, "UTC", ms("2026-01-10T07:00:00Z")).unwrap(),
        "2026-01-10T08:00:00.000Z"
    );
}
#[test]
fn a_collection_no_collector_schedules_is_refused() {
    crate::testing::install(&[], &[]);
    let (_root, db, id) = crate::testing::bootstrapped();
    for collection in ["next", "", "paycom.collect"] {
        let value = json!({"name":"Collection","collection":collection,"cadence":"interval",
            "intervalMinutes":120,"localTime":"00:00","enabled":false});
        let refused = db.save_schedule(&id, None, &value).unwrap_err();
        assert_eq!(
            (refused.code.as_str(), refused.status),
            ("invalid_input", 400)
        );
    }
    assert!(db.collection_schedules(&id).unwrap().schedules.is_empty());
}
