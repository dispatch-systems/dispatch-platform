//! Where someone stands in each category: the rules a writeup's tier climbs by. Pure, so the
//! rules are tested apart from storage.
use crate::api::types::{ConductStanding, ConductTier, ConductWriteupRow};
use chrono::{Months, NaiveDate};
use std::collections::BTreeMap;

/// The first day a writeup dated on or after counts toward the next tier: `months` before
/// `today`, the same day of the month, or the month's last where it has none.
pub(crate) fn counts_from(today: NaiveDate, months: u32) -> NaiveDate {
    today
        .checked_sub_months(Months::new(months))
        .unwrap_or(NaiveDate::MIN)
}

/// Each person's highest tier in each category among the writeups dated `from` on, held by the
/// latest writeup at that tier. `writeups` are the newest first, as the overview lists them.
pub(crate) fn standings(writeups: &[ConductWriteupRow], from: &str) -> Vec<ConductStanding> {
    let mut held: BTreeMap<(&str, &str), &ConductWriteupRow> = BTreeMap::new();
    for writeup in writeups.iter().filter(|w| w.written_on.as_str() >= from) {
        let key = (writeup.person.as_str(), writeup.category_id.as_str());
        // The newest come first, so a later one at the same tier never replaces it.
        match held.get(&key) {
            Some(kept) if rank(kept.tier) >= rank(writeup.tier) => {}
            _ => {
                held.insert(key, writeup);
            }
        }
    }
    held.into_values()
        .map(|w| ConductStanding {
            person: w.person.clone(),
            category_id: w.category_id.clone(),
            tier: w.tier,
            writeup_id: w.id.clone(),
            written_on: w.written_on.clone(),
        })
        .collect()
}

fn rank(tier: ConductTier) -> usize {
    ConductTier::ALL
        .iter()
        .position(|t| *t == tier)
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "../tests/backend/standing.rs"]
mod tests;
