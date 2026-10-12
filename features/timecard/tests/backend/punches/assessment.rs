use super::*;
#[test]
fn collected_clock_syntax_keeps_legacy_whitespace_and_hour_rules() {
    for (text, minutes) in [
        ("12:00 AM", Some(0)),
        ("12:00 pm", Some(720)),
        ("1:05 pm", Some(785)),
        ("\u{feff}08:00\u{00a0}", Some(480)),
        ("\u{0085}08:00", None),
        ("24:00", None),
        ("12:60", None),
        ("8:0", None),
        ("08:00:00", None),
        ("00:00 AM", None),
    ] {
        assert_eq!(parse_clock(text), minutes, "{text:?}");
    }
}

#[test]
fn daily_work_splits_at_eight_net_hours_and_keeps_incomplete_punches_unknown() {
    use crate::api::types::{InPunchKind, OutPunchKind};
    let cases: &[(&str, &[(&str, &str)], Option<(i32, i32)>)] = &[
        (
            "under eight",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "12:00"),
                ("IN LUNCH", "12:30"),
                ("OUT DAY", "16:00"),
            ],
            Some((450, 0)),
        ),
        (
            "exactly eight",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "12:00"),
                ("IN LUNCH", "12:30"),
                ("OUT DAY", "16:30"),
            ],
            Some((480, 0)),
        ),
        (
            "one minute over",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "12:00"),
                ("IN LUNCH", "12:30"),
                ("OUT DAY", "16:31"),
            ],
            Some((480, 1)),
        ),
        (
            "long meal",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "12:00"),
                ("IN LUNCH", "14:00"),
                ("OUT DAY", "18:00"),
            ],
            Some((480, 0)),
        ),
        (
            "multiple meals",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "11:00"),
                ("IN LUNCH", "11:30"),
                ("OUT LUNCH", "14:00"),
                ("IN LUNCH", "14:45"),
                ("OUT DAY", "18:00"),
            ],
            Some((480, 45)),
        ),
        (
            "separate shifts",
            &[
                ("IN DAY", "08:00"),
                ("OUT DAY", "12:00"),
                ("IN DAY", "13:00"),
                ("OUT DAY", "18:00"),
            ],
            Some((480, 60)),
        ),
        (
            "overnight",
            &[
                ("IN DAY", "20:00"),
                ("OUT LUNCH", "00:00"),
                ("IN LUNCH", "00:30"),
                ("OUT DAY", "05:00"),
            ],
            Some((480, 30)),
        ),
        (
            "no meal",
            &[("IN DAY", "08:00"), ("OUT DAY", "17:00")],
            Some((480, 60)),
        ),
        (
            "missing lunch return",
            &[("IN DAY", "08:00"), ("OUT LUNCH", "12:00")],
            None,
        ),
        (
            "missing end",
            &[
                ("IN DAY", "08:00"),
                ("OUT LUNCH", "12:00"),
                ("IN LUNCH", "12:30"),
            ],
            None,
        ),
        (
            "unpaired return",
            &[
                ("IN DAY", "08:00"),
                ("IN LUNCH", "12:30"),
                ("OUT DAY", "17:00"),
            ],
            None,
        ),
        (
            "end before start",
            &[
                ("OUT DAY", "08:00"),
                ("IN DAY", "09:00"),
                ("OUT DAY", "17:00"),
            ],
            None,
        ),
        (
            "unreadable clock",
            &[("IN DAY", "unknown"), ("OUT DAY", "17:00")],
            None,
        ),
        ("reported leave without work punches", &[], None),
    ];
    for (name, events, expected) in cases {
        let punches = events
            .iter()
            .map(|(kind, clock)| {
                let mut punch = Punch {
                    in_kind: None,
                    out_kind: None,
                    clock_in: None,
                    clock_out: None,
                    hours: None,
                };
                match *kind {
                    "IN DAY" | "IN LUNCH" => {
                        punch.in_kind = Some(Some(if *kind == "IN DAY" {
                            InPunchKind::Day
                        } else {
                            InPunchKind::Lunch
                        }));
                        punch.clock_in = Some((*clock).into());
                    }
                    _ => {
                        punch.out_kind = Some(Some(if *kind == "OUT DAY" {
                            OutPunchKind::Day
                        } else {
                            OutPunchKind::Lunch
                        }));
                        punch.clock_out = Some((*clock).into());
                    }
                }
                punch
            })
            .collect();
        let card = Timecard {
            employee_code: "fixture-employee".into(),
            date: "2026-10-04".into(),
            // Deliberately differs from clocked work: includes reported non-work time.
            hours: 12.0,
            status: "Complete".into(),
            punches,
            source_url: None,
        }
        .assessed("America/Los_Angeles");
        assert_eq!(
            card.regular_minutes.zip(card.overtime_minutes),
            *expected,
            "{name}"
        );
        if *name == "no meal" {
            let unresolved = Timecard {
                status: "Missing punch".into(),
                ..card.card
            }
            .assessed("America/Los_Angeles");
            assert_eq!(
                (unresolved.regular_minutes, unresolved.overtime_minutes),
                (None, None),
                "a missing-punch flag remains unresolved despite a closed visible segment"
            );
        }
    }
    let empty = Timecard {
        employee_code: "fixture-employee".into(),
        date: "2026-10-05".into(),
        hours: 0.0,
        status: "Complete".into(),
        punches: vec![],
        source_url: None,
    }
    .assessed("America/Los_Angeles");
    assert_eq!(
        (empty.regular_minutes, empty.overtime_minutes),
        (Some(0), Some(0))
    );
}

#[test]
fn complete_legacy_pairs_count_work_without_deducting_meals_twice() {
    let card = Timecard {
        employee_code: "fixture-employee".into(),
        date: "2026-10-04".into(),
        hours: 8.5,
        status: "Complete".into(),
        source_url: None,
        punches: [("08:00", "12:00"), ("12:30", "17:00")]
            .into_iter()
            .map(|(start, end)| Punch {
                in_kind: None,
                out_kind: None,
                clock_in: Some(start.into()),
                clock_out: Some(end.into()),
                hours: None,
            })
            .collect(),
    }
    .assessed("America/Los_Angeles");
    assert_eq!(
        (card.regular_minutes, card.overtime_minutes),
        (Some(480), Some(30))
    );
    for (date, meal_out, expected) in [
        ("2026-11-01", "03:00", Some((480, 90))),
        ("2026-03-08", "03:00", Some((450, 0))),
        ("2026-11-01", "01:30", None),
        ("2026-03-08", "02:30", None),
    ] {
        let mut shifted = card.card.clone();
        shifted.date = date.into();
        shifted.punches[0].clock_in = Some("00:00".into());
        shifted.punches[0].clock_out = Some(meal_out.into());
        shifted.punches[1].clock_in = Some("03:30".into());
        shifted.punches[1].clock_out = Some("09:00".into());
        let assessed = shifted.assessed("America/Los_Angeles");
        assert_eq!(
            assessed.regular_minutes.zip(assessed.overtime_minutes),
            expected,
            "{date} meal out {meal_out}: elapsed work across a DST transition"
        );
    }
}
