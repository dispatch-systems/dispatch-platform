//! The pairs that may be one person: who they are, why, and which keeps their code.
use super::IdRow;
use super::matching::{AMAZON, Saved};
use super::overview::Found;
use super::review;
use super::sources::{Days, Identity};
use crate::api::types::{Driver, DriverEvidence, DriverRole, DriverStrength};
use dispatch_core::{foundation::names, manifest::people::DriverSource};
use std::collections::{BTreeMap, BTreeSet};

/// Two people who may be one: `person` would join `candidate`, keeping `candidate`'s code.
pub(super) struct Pair {
    pub strength: DriverStrength,
    pub person: String,
    pub candidate: String,
    pub evidence: Vec<DriverEvidence>,
}

/// The days a person worked, and the days the sources that know them were collected for.
fn worked(
    ids: &[(&IdRow, DriverSource)],
    days: &Days,
) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    let mut worked = BTreeSet::new();
    let mut collected = BTreeSet::new();
    let mut any = false;
    for (row, source) in ids {
        if let Some(set) = days.of(&(*source, row.external_id.clone())) {
            worked.extend(set.iter().cloned());
            any = true;
        }
        if let Some(set) = days.collected.get(source) {
            collected.extend(set.iter().cloned());
        }
    }
    any.then_some((worked, collected))
}

/// The pairs that may be one person, the strongest first, and no more than three choices for
/// any one person. Each pair's `person` holds fewer IDs, or a source later in the order:
/// joining them keeps the `candidate`'s code, so a code follows the employment record.
pub(super) fn pairs(
    held: &BTreeMap<&str, Vec<(&IdRow, DriverSource)>>,
    drivers: &BTreeMap<String, Driver>,
    found: &Found,
    days: &Days,
    saved: &Saved,
    apart: &BTreeSet<(String, String)>,
) -> Vec<Pair> {
    // Someone kept apart from Paycom on the meal-break page; that decision stands.
    let kept = |ids: &[(&IdRow, DriverSource)]| {
        ids.iter().any(|(row, source)| {
            source.as_str() == AMAZON && saved.separate.contains(&row.external_id)
        })
    };
    // Nobody asks about someone who has left; back on a roster or a route, they are asked again.
    let sides: Vec<review::Side> = held
        .iter()
        .filter(|(code, ids)| !kept(ids) && drivers[**code].role != DriverRole::Former)
        .map(|(code, ids)| {
            let described: Vec<&Identity> = ids
                .iter()
                .filter_map(|(row, source)| found.get(&(*source, row.external_id.clone())))
                .collect();
            review::Side {
                code,
                sources: ids.iter().map(|(_, source)| *source).collect(),
                // Every name an ID's sources write, so a middle name or a different spelling
                // in one of them still counts.
                names: described
                    .iter()
                    .flat_map(|x| x.names.iter().map(String::as_str))
                    .collect(),
                emails: described
                    .iter()
                    .flat_map(|x| x.emails.iter().map(String::as_str))
                    .collect(),
                phones: described
                    .iter()
                    .flat_map(|x| x.phones.iter().map(String::as_str))
                    .collect(),
            }
        })
        .collect();
    let mut pairs: Vec<Pair> = review::suggest(&sides, apart)
        .into_iter()
        .map(|mut suggestion| {
            let order = |code: &str| {
                let ids = &held[code];
                let latest = ids.iter().map(|(_, source)| *source).max();
                (std::cmp::Reverse(ids.len()), latest)
            };
            let (person, candidate) = if order(&suggestion.first) > order(&suggestion.second) {
                (suggestion.first, suggestion.second)
            } else {
                (suggestion.second, suggestion.first)
            };
            if let (Some((drove, drove_days)), Some((clocked, clocked_days))) = (
                worked(&held[person.as_str()], days),
                worked(&held[candidate.as_str()], days),
            ) && let Some(days) =
                review::worked_days(Some(&clocked), Some(&drove), &clocked_days, &drove_days)
            {
                suggestion.evidence.push(days);
            }
            Pair {
                strength: review::strength(&suggestion.evidence),
                person,
                candidate,
                evidence: suggestion.evidence,
            }
        })
        .collect();
    pairs.sort_by(|a, b| {
        (a.strength != DriverStrength::Strong)
            .cmp(&(b.strength != DriverStrength::Strong))
            .then(b.evidence.len().cmp(&a.evidence.len()))
            .then_with(|| names::compare(&drivers[&a.person].name, &drivers[&b.person].name))
    });
    let mut shown: BTreeMap<String, usize> = BTreeMap::new();
    pairs.retain(|pair| {
        let count = shown.entry(pair.person.clone()).or_default();
        *count += 1;
        *count <= 3
    });
    pairs
}
