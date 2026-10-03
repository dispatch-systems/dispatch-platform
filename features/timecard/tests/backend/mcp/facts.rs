use super::*;
use serde_json::json;

fn card(punches: Value) -> TimecardDay {
    timecard_day(
        serde_json::from_value(json!({
            "employeeCode":"E1", "date":"2026-09-12", "name":"Driver",
            "hours":8, "status":"Complete", "punches":punches,
        }))
        .unwrap(),
    )
}

#[test]
fn lunch_totals_distinguish_missing_open_mixed_complete_and_zero_durations() {
    let cases = [
        (json!([{"in":"08:00","out":"17:00"}]), None),
        (
            json!([{"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"}]),
            None,
        ),
        (
            json!([
                {"in":"08:00","inKind":"IN DAY","out":"12:00","outKind":"OUT LUNCH"},
                {"in":"12:30","inKind":"IN LUNCH","out":"14:00","outKind":"OUT LUNCH"}
            ]),
            None,
        ),
        (
            json!([{"in":"08:00","out":"12:00"},{"in":"12:30","out":"17:00"}]),
            Some(30),
        ),
        (
            json!([{"in":"08:00","out":"12:00"},{"in":"12:00","out":"17:00"}]),
            Some(0),
        ),
    ];
    for (punches, expected) in cases {
        assert_eq!(card(punches.clone()).lunch_minutes, expected, "{punches}");
    }
    assert_eq!(total_minutes([Some(30), Some(15)].into_iter()), Some(45));
    assert_eq!(total_minutes([Some(30), None].into_iter()), None);
    assert_eq!(total_minutes([None].into_iter()), None);
    assert_eq!(total_minutes([].into_iter()), None);
}

#[test]
fn overnight_lunch_uses_elapsed_minutes_and_local_clock_labels() {
    let card = card(json!([
        {"in":"22:00","inKind":"IN DAY","out":"23:50","outKind":"OUT LUNCH"},
        {"in":"00:20","inKind":"IN LUNCH","out":"06:00","outKind":"OUT DAY"}
    ]));
    assert_eq!(card.lunch_minutes, Some(30));
    assert_eq!(card.clock_in.as_deref(), Some("22:00"));
    assert_eq!(card.clock_out.as_deref(), Some("06:00 +1 day"));
    assert!(!card.needs_review);
}
