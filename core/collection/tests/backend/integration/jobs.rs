use dispatch_backend::schedules;

#[test]
fn schedule_handles_dst_gaps_and_repeated_minutes() {
    let parse = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .timestamp_millis()
    };
    assert_eq!(
        schedules::next_daily("02:30", "America/Chicago", parse("2026-03-08T07:59:00Z")).unwrap(),
        "2026-03-09T07:30:00.000Z"
    );
    assert_eq!(
        schedules::next_daily("01:30", "America/Chicago", parse("2026-11-01T06:30:00Z")).unwrap(),
        "2026-11-02T07:30:00.000Z"
    );
    assert!(schedules::next_daily("25:99", "UTC", 0).is_err());
}
