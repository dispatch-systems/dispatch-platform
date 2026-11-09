//! The rules a category's tiers climb by.
use super::*;
use crate::api::types::ConductWriteupRow;

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

#[test]
fn a_writeup_counts_for_the_months_the_dsp_chose() {
    let day = |text| NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap();
    assert_eq!(counts_from(day("2026-11-08"), 12), day("2025-11-08"));
    // A month without the day counts from its last.
    assert_eq!(counts_from(day("2026-05-31"), 3), day("2026-02-28"));
}

#[test]
fn each_person_stands_at_their_highest_tier_in_each_category_that_still_counts() {
    use ConductTier::{Final, Verbal, Written};
    // The newest first, as the overview lists them.
    let writeups = [
        writeup("w6", "ana", "tardies", Verbal, "2026-10-01"),
        writeup("w5", "ana", "safety", Written, "2026-09-01"),
        writeup("w4", "ana", "tardies", Written, "2026-08-01"),
        writeup("w3", "ana", "tardies", Written, "2026-07-01"),
        writeup("w2", "ben", "tardies", Final, "2025-06-01"),
        writeup("w1", "ben", "tardies", Verbal, "2026-01-01"),
    ];
    let standings = standings(&writeups, "2025-11-08");
    let held: Vec<(&str, &str, ConductTier, &str)> = standings
        .iter()
        .map(|s| {
            (
                s.person.as_str(),
                s.category_id.as_str(),
                s.tier,
                s.writeup_id.as_str(),
            )
        })
        .collect();
    assert_eq!(
        held,
        [
            // A lower tier after a higher one doesn't bring it down, and the latest at the
            // higher tier holds it.
            ("ana", "safety", Written, "w5"),
            ("ana", "tardies", Written, "w4"),
            // A final from before the months that count no longer does.
            ("ben", "tardies", Verbal, "w1"),
        ]
    );
}
