//! The pairs that may be one person: who they are, why, and which keeps their code.
use super::IdRow;
use super::review::{self, Side, Suggestion};
use super::sources::{Days, Found, Identity};
use crate::api::types::{Driver, DriverEvidence, DriverRole, DriverStrength};
use dispatch_core::{foundation::names, manifest::people::DriverSource};
use std::collections::{BTreeMap, BTreeSet};

/// How many pairs any one person is asked about at once.
const MOST_PER_PERSON: usize = 3;

/// Two people who may be one: `person` would join `candidate`, keeping `candidate`'s code.
pub(super) struct Pair {
    pub strength: DriverStrength,
    pub person: String,
    pub candidate: String,
    pub evidence: Vec<DriverEvidence>,
}

/// Each person's IDs, by their code, with each ID's source, in the sources' order.
type Held<'a> = BTreeMap<&'a str, Vec<(&'a IdRow, DriverSource)>>;

/// The pairs that may be one person, the strongest first, and no more than
/// `MOST_PER_PERSON` choices for any one person. Nobody is asked about someone who has left;
/// back on a roster or a route, they are asked again.
pub(super) fn pairs(
    held: &Held,
    drivers: &BTreeMap<String, Driver>,
    found: &Found,
    days: &Days,
    apart: &BTreeSet<(String, String)>,
) -> Vec<Pair> {
    let sides: Vec<Side> = held
        .iter()
        .map(|(code, ids)| side(code, ids, found, drivers[*code].role != DriverRole::Former))
        .collect();
    let pairs = review::suggest(&sides, apart)
        .into_iter()
        .map(|suggestion| pair(suggestion, held, days))
        .collect();
    ranked(pairs, |code| &drivers[code].name)
}

/// One person as review compares them.
fn side<'a>(
    code: &'a str,
    ids: &[(&IdRow, DriverSource)],
    found: &'a Found,
    asked: bool,
) -> Side<'a> {
    let described: Vec<&Identity> = ids
        .iter()
        .filter_map(|(row, source)| found.get(&(*source, row.external_id.clone())))
        .collect();
    Side {
        code,
        // A list of people the DSP can reach may list one person twice, as an account in each
        // of two workspaces, so it never keeps two people apart.
        sources: ids
            .iter()
            .filter(|(row, source)| {
                found
                    .get(&(*source, row.external_id.clone()))
                    .is_none_or(|x| !x.contacts_only)
            })
            .map(|(_, source)| *source)
            .collect(),
        // Every name an ID's sources write, so a middle name or a different spelling in one of
        // them still counts.
        names: described
            .iter()
            .flat_map(|x| x.names.iter().map(String::as_str))
            .collect(),
        contacts: described.iter().flat_map(|x| x.contacts.all()).collect(),
        asked,
    }
}

/// A suggestion as a pair: the one holding fewer IDs, or a source later in the order, joins
/// the other, so a code follows the employment record; with how many of the days one worked
/// the other did too.
fn pair(mut suggestion: Suggestion, held: &Held, days: &Days) -> Pair {
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
    if let (Some((theirs, their_days)), Some((others, other_days))) = (
        worked(&held[person.as_str()], days),
        worked(&held[candidate.as_str()], days),
    ) && let Some(days) = review::worked_days(&theirs, &others, &their_days, &other_days)
    {
        suggestion.evidence.push(days);
    }
    Pair {
        strength: review::strength(&suggestion.evidence),
        person,
        candidate,
        evidence: suggestion.evidence,
    }
}

/// The days a person worked, and the days the sources that know them were collected for:
/// none while no source says when they worked.
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

/// The pairs in the order they're asked: strong first, then with more reasons, then by the
/// joining person's name, `MOST_PER_PERSON` at most for each person joining.
fn ranked<'a>(mut pairs: Vec<Pair>, name: impl Fn(&str) -> &'a str) -> Vec<Pair> {
    pairs.sort_by(|a, b| {
        (a.strength != DriverStrength::Strong)
            .cmp(&(b.strength != DriverStrength::Strong))
            .then(b.evidence.len().cmp(&a.evidence.len()))
            .then_with(|| names::compare(name(&a.person), name(&b.person)))
    });
    let mut asked: BTreeMap<String, usize> = BTreeMap::new();
    pairs.retain(|pair| {
        let count = asked.entry(pair.person.clone()).or_default();
        *count += 1;
        *count <= MOST_PER_PERSON
    });
    pairs
}

#[cfg(test)]
#[path = "../tests/backend/pairs.rs"]
mod tests;
