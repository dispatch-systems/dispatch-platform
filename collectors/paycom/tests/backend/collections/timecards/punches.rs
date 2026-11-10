use super::*;
use chrono_tz::America::Chicago;

fn period() -> EmployeeTimecardPeriod {
    EmployeeTimecardPeriod {
        from: "2026-09-06".into(),
        to: "2026-09-19".into(),
    }
}
/// A punch at a Chicago clock time, as the API sends one: in seconds since the epoch.
fn punch(date: &str, clock: &str, kind: &str) -> Value {
    let at = Chicago
        .from_local_datetime(
            &NaiveDateTime::parse_from_str(&format!("{date} {clock}"), "%Y-%m-%d %H:%M").unwrap(),
        )
        .unwrap();
    json!({"entrytype":"1","punchtime":at.timestamp().to_string(),"punchtype":kind,"isdeleted":0})
}
fn day<'a>(cards: &'a [Value], date: &str) -> &'a Value {
    cards.iter().find(|card| card["date"] == date).unwrap()
}
fn shown(card: &Value) -> Vec<(Value, Value, Value, Value)> {
    card["punches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["in"].clone(),
                p["inKind"].clone(),
                p["out"].clone(),
                p["outKind"].clone(),
            )
        })
        .collect()
}

#[test]
fn punches_become_a_periods_cards_paired_on_the_day_their_shift_began() {
    let rows = [
        // Sent out of order: a full day with a lunch.
        punch("2026-09-07", "17:00", "OD"),
        punch("2026-09-07", "08:00", "ID"),
        punch("2026-09-07", "12:00", "OL"),
        punch("2026-09-07", "12:30", "IL"),
        // A shift that ends after midnight, with a break, stays on the day it began.
        punch("2026-09-08", "22:00", "ID"),
        punch("2026-09-09", "01:00", "OB"),
        punch("2026-09-09", "01:15", "IB"),
        punch("2026-09-09", "06:00", "OD"),
        // Clocked in and never out, then a new day: the open shift takes nothing with it.
        punch("2026-09-10", "08:00", "ID"),
        punch("2026-09-12", "09:00", "ID"),
        punch("2026-09-12", "10:30", "OD"),
        // A punch that was deleted, and eight hours of leave entered without one.
        json!({"entrytype":1,"punchtime":"2026-09-14 08:00:00","punchtype":"ID","isdeleted":1}),
        json!({"entrytype":"2","punchtime":"2026-09-14 00:00:00","hours":"8.00","isdeleted":"0"}),
        // The evening before the period, which belongs to the one before.
        punch("2026-09-05", "22:00", "ID"),
        punch("2026-09-06", "02:00", "OD"),
    ];
    let cards = cards(&rows, "A0B1", &period(), Chicago).unwrap();
    assert_eq!(cards.len(), 14);
    assert!(cards.iter().all(|card| card["employeeCode"] == "A0B1"));
    let text = |value: &str| json!(value);
    let full = day(&cards, "2026-09-07");
    assert_eq!(
        (&full["hours"], &full["status"]),
        (&json!(8.5), &json!("Complete"))
    );
    assert_eq!(
        shown(full),
        [
            (
                text("08:00 AM"),
                text("IN DAY"),
                text("12:00 PM"),
                text("OUT LUNCH")
            ),
            (
                text("12:30 PM"),
                text("IN LUNCH"),
                text("05:00 PM"),
                text("OUT DAY")
            ),
        ]
    );
    let night = day(&cards, "2026-09-08");
    assert_eq!(
        (&night["hours"], &night["status"]),
        (&json!(7.75), &json!("Complete"))
    );
    assert_eq!(shown(night)[1].2, "06:00 AM");
    assert_eq!(day(&cards, "2026-09-09")["status"], "No punches");
    let open = day(&cards, "2026-09-10");
    assert_eq!(
        (&open["hours"], &open["status"]),
        (&json!(0.0), &json!("Missing punch"))
    );
    assert_eq!(
        shown(open),
        [(text("08:00 AM"), text("IN DAY"), Value::Null, Value::Null)]
    );
    assert_eq!(day(&cards, "2026-09-12")["hours"], 1.5);
    let leave = day(&cards, "2026-09-14");
    assert_eq!(
        (&leave["hours"], &leave["status"]),
        (&json!(8.0), &json!("No punches"))
    );
    assert_eq!(day(&cards, "2026-09-06")["status"], "No punches");
}

#[test]
fn a_punch_it_cannot_place_fails_the_read_rather_than_becoming_a_wrong_card() {
    for row in [
        // A kind it doesn't know, a time in milliseconds, one that is no time, and no type.
        punch("2026-09-07", "08:00", "TR"),
        json!({"entrytype":1,"punchtime":1_788_786_000_000_i64,"punchtype":"ID"}),
        json!({"entrytype":1,"punchtime":"yesterday","punchtype":"ID"}),
        json!({"punchtime":"2026-09-07 08:00:00","punchtype":"ID"}),
    ] {
        let refused = cards(std::slice::from_ref(&row), "A0B1", &period(), Chicago).unwrap_err();
        assert!(refused.is(codes::API_UNREADABLE), "{row}");
    }
}
