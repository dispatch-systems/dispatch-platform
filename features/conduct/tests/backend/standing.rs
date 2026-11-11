//! The rules a category's tiers climb and step down by.
use super::*;
use ConductTier::{Final, Verbal, Written};

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn writeup(
    id: &str,
    person: &str,
    category: &str,
    tier: ConductTier,
    on: &str,
) -> ConductWriteupRow {
    ConductWriteupRow {
        id: id.into(),
        person: person.into(),
        person_name: person.into(),
        category_id: category.into(),
        tier,
        final_kind: None,
        suspension: None,
        written_on: on.into(),
        supervisor_name: "Supervisor".into(),
        signed_on: None,
        has_copy: false,
        revision: 1,
    }
}

fn step(kind: ConductStepKind, after: i64, unit: ConductUnit) -> Option<ConductStepDown> {
    Some(ConductStepDown { kind, after, unit })
}

fn category(id: &str, step_down: Option<ConductStepDown>) -> ConductCategory {
    ConductCategory {
        id: id.into(),
        name: id.into(),
        start_tier: Verbal,
        step_down,
        statement: String::new(),
        prior: String::new(),
        plan: String::new(),
        revision: 1,
        retired: false,
    }
}

#[test]
fn a_category_reduces_a_tier_at_a_time_or_resets_and_a_writeup_starts_the_wait_again() {
    use ConductStepKind::{Reduce, Reset};
    use ConductUnit::{Months, Weeks};
    let categories = [
        // One tier at a time, six months at each.
        category("tardies", step(Reduce, 6, Months)),
        // Straight to a clean record two weeks on.
        category("damage", step(Reset, 2, Weeks)),
        // Never steps down.
        category("safety", None),
    ];
    // The newest first, as the overview lists them.
    let writeups = [
        writeup("c1", "cy", "tardies", Final, "2026-10-01"),
        writeup("b2", "ben", "tardies", Verbal, "2026-10-01"),
        writeup("a5", "ana", "damage", Final, "2026-10-20"),
        writeup("a2", "ana", "tardies", Written, "2026-03-01"),
        writeup("a4", "ana", "safety", Verbal, "2025-02-01"),
        writeup("b1", "ben", "tardies", Final, "2025-01-15"),
        writeup("a1", "ana", "tardies", Verbal, "2026-01-10"),
        writeup("a3", "ana", "safety", Written, "2025-01-01"),
    ];
    let worked = work_out(&categories, &writeups, day("2026-11-08"));
    let held: Vec<_> = worked
        .standings
        .iter()
        .map(|s| {
            (
                s.person.as_str(),
                s.category_id.as_str(),
                s.tier,
                s.latest_id.as_str(),
                s.steps_down_on.as_deref(),
                s.steps_down_to,
            )
        })
        .collect();
    assert_eq!(
        held,
        [
            // A lower tier after a higher one keeps the higher, which never steps down here.
            ("ana", "safety", Written, "a4", None, None),
            // Written on 3/1, a Verbal from 9/1, clean six months on.
            ("ana", "tardies", Verbal, "a2", Some("2027-03-01"), None),
            // Ben's Final stepped all the way down to clean before his Verbal, which starts over.
            ("ben", "tardies", Verbal, "b2", Some("2027-04-01"), None),
            // A Final reduces to Written first.
            (
                "cy",
                "tardies",
                Final,
                "c1",
                Some("2027-04-01"),
                Some(Written)
            ),
            // Ana's damage Final cleared two weeks on, on 11/3.
        ]
    );
    // Where each writeup's person stood just before it: none while their record was clean.
    assert_eq!(
        worked
            .stood_before
            .iter()
            .map(|(id, tier)| (id.as_str(), *tier))
            .collect::<Vec<_>>(),
        [("a2", Verbal), ("a4", Written)]
    );
}

#[test]
fn a_wait_in_months_ends_on_the_same_day_or_the_months_last() {
    let six = step(ConductStepKind::Reduce, 6, ConductUnit::Months).unwrap();
    assert_eq!(due(day("2026-03-01"), &six), Some(day("2026-09-01")));
    assert_eq!(due(day("2026-08-31"), &six), Some(day("2027-02-28")));
    let ten = step(ConductStepKind::Reset, 10, ConductUnit::Days).unwrap();
    assert_eq!(due(day("2026-12-25"), &ten), Some(day("2027-01-04")));
}

#[test]
fn a_historical_record_excludes_later_notices_and_steps_down_at_the_selected_date() {
    let categories = [category(
        "safety",
        step(ConductStepKind::Reduce, 6, ConductUnit::Months),
    )];
    let rows = [
        writeup("later", "ana", "safety", Final, "2026-11-01"),
        writeup("older", "ana", "safety", Written, "2026-01-01"),
    ];
    let historical = work_out(&categories, &rows, day("2026-10-01"));
    assert_eq!(historical.standings[0].tier, Verbal);
    assert_eq!(historical.standings[0].latest_id, "older");
    assert!(!historical.stood_before.contains_key("later"));
    assert_eq!(
        work_out(&categories, &rows, day("2026-11-08")).standings[0].tier,
        Final
    );
}
