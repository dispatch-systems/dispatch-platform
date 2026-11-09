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

fn step(
    tier: ConductTier,
    after: i64,
    unit: ConductUnit,
    to: Option<ConductTier>,
) -> ConductStepDown {
    ConductStepDown {
        tier,
        after,
        unit,
        to,
    }
}

fn category(id: &str, step_downs: Vec<ConductStepDown>) -> ConductCategory {
    ConductCategory {
        id: id.into(),
        name: id.into(),
        start_tier: Verbal,
        step_downs,
        statement: String::new(),
        prior: String::new(),
        plan: String::new(),
        revision: 1,
        retired: false,
    }
}

#[test]
fn each_tier_steps_down_as_its_category_says_and_a_writeup_starts_the_wait_again() {
    use ConductUnit::{Months, Weeks};
    let categories = [
        // One tier at a time, six months at each.
        category(
            "tardies",
            vec![
                step(Final, 6, Months, Some(Written)),
                step(Written, 6, Months, Some(Verbal)),
                step(Verbal, 6, Months, None),
            ],
        ),
        // Straight from Final to a clean record.
        category("damage", vec![step(Final, 2, Weeks, None)]),
        // Never steps down.
        category("safety", vec![]),
    ];
    // The newest first, as the overview lists them.
    let writeups = [
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
    let six = step(Written, 6, ConductUnit::Months, Some(Verbal));
    assert_eq!(due(day("2026-03-01"), &six), Some(day("2026-09-01")));
    assert_eq!(due(day("2026-08-31"), &six), Some(day("2027-02-28")));
    let ten = step(Final, 10, ConductUnit::Days, None);
    assert_eq!(due(day("2026-12-25"), &ten), Some(day("2027-01-04")));
}
