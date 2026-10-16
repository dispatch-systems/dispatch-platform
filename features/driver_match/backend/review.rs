//! Who might be one person listed twice: two people no source knows both of, whose names
//! come close without matching, or any two who alone share an email address or phone
//! number. A person decides; this only gathers the reasons.
use super::names::{Name, capitalized, short_form};
use crate::api::types::{DriverEvidence, DriverEvidenceKind, DriverStrength};
use dispatch_core::manifest::people::DriverSource;
use std::collections::{BTreeMap, BTreeSet};

/// One person who might be another: their code, the sources that know them, and every
/// name and contact their IDs carry.
pub(crate) struct Side<'a> {
    pub code: &'a str,
    pub sources: BTreeSet<DriverSource>,
    pub names: Vec<&'a str>,
    pub emails: BTreeSet<&'a str>,
    pub phones: BTreeSet<&'a str>,
}
pub(crate) struct Suggestion {
    pub first: String,
    pub second: String,
    pub evidence: Vec<DriverEvidence>,
}

fn evidence(kind: DriverEvidenceKind) -> DriverEvidence {
    DriverEvidence {
        kind,
        short: None,
        long: None,
        same: None,
        of: None,
    }
}

/// The reasons two names might be one person's, or none when nothing ties them: a shared
/// last name, or a first name and a last initial.
fn reasons(a: &Name, b: &Name) -> Vec<DriverEvidence> {
    let (Some(last_a), Some(last_b)) = (a.last(), b.last()) else {
        return vec![];
    };
    let same_last = last_a == last_b && last_a.chars().count() > 1;
    // Amazon sometimes writes only a last initial, as "Jordan K."
    let initial = !same_last
        && (last_a.chars().count() == 1 || last_b.chars().count() == 1)
        && last_a.chars().next() == last_b.chars().next();
    let first = if a.given == b.given && !a.given.is_empty() {
        Some(evidence(DriverEvidenceKind::SameFirstName))
    } else {
        short_form(&a.given, &b.given).map(|(short, long)| DriverEvidence {
            short: Some(capitalized(short)),
            long: Some(capitalized(long)),
            ..evidence(DriverEvidenceKind::ShortForm)
        })
    };
    let tied = same_last || (initial && first.is_some());
    if !tied {
        return vec![];
    }
    let mut out = vec![evidence(if same_last {
        DriverEvidenceKind::SameLastName
    } else {
        DriverEvidenceKind::LastInitial
    })];
    out.extend(first);
    out
}

/// Every pair worth a look, with the contacts only the two of them share and the strongest
/// reasons their names give. Two people one source knows both of are a pair only by a
/// shared contact, as a rehire under a second ID is. `apart` holds the pairs someone already
/// decided are different people, as (first, second) codes.
pub(crate) fn suggest(people: &[Side<'_>], apart: &BTreeSet<(String, String)>) -> Vec<Suggestion> {
    // How many people carry each contact: one that more than two carry says nothing.
    let mut carried: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    for side in people {
        for email in &side.emails {
            *carried.entry((true, email)).or_default() += 1;
        }
        for phone in &side.phones {
            *carried.entry((false, phone)).or_default() += 1;
        }
    }
    let names: Vec<Vec<Name>> = people
        .iter()
        .map(|side| side.names.iter().map(|n| Name::new(n)).collect())
        .collect();
    let mut out = vec![];
    for (i, a) in people.iter().enumerate() {
        for (j, b) in people.iter().enumerate().skip(i + 1) {
            let key = if a.code < b.code {
                (a.code.to_owned(), b.code.to_owned())
            } else {
                (b.code.to_owned(), a.code.to_owned())
            };
            if apart.contains(&key) {
                continue;
            }
            let shared = |mine: &BTreeSet<&str>, theirs: &BTreeSet<&str>, email: bool| {
                mine.intersection(theirs)
                    .any(|value| carried.get(&(email, *value)) == Some(&2))
            };
            let mut found = vec![];
            if shared(&a.emails, &b.emails, true) {
                found.push(evidence(DriverEvidenceKind::SameEmail));
            }
            if shared(&a.phones, &b.phones, false) {
                found.push(evidence(DriverEvidenceKind::SamePhone));
            }
            if found.is_empty() && !a.sources.is_disjoint(&b.sources) {
                continue;
            }
            let best = names[i]
                .iter()
                .flat_map(|x| names[j].iter().map(move |y| reasons(x, y)))
                .max_by_key(|r| {
                    (
                        r.len(),
                        r.iter().any(|e| e.kind == DriverEvidenceKind::SameLastName),
                    )
                })
                .unwrap_or_default();
            found.extend(best);
            if !found.is_empty() {
                out.push(Suggestion {
                    first: a.code.to_owned(),
                    second: b.code.to_owned(),
                    evidence: found,
                });
            }
        }
    }
    out
}

/// The days both sides worked out of the days one side worked, where both sides' sources
/// were collected: of the days someone drove, the days the other clocked in. `None` with
/// fewer than two such days, which say nothing.
pub(crate) fn worked_days(
    clocked: Option<&BTreeSet<String>>,
    drove: Option<&BTreeSet<String>>,
    clocked_days: &BTreeSet<String>,
    drove_days: &BTreeSet<String>,
) -> Option<DriverEvidence> {
    let drove = drove?;
    let window: BTreeSet<&String> = clocked_days.intersection(drove_days).collect();
    let days: Vec<&String> = drove.iter().filter(|d| window.contains(d)).collect();
    if days.len() < 2 {
        return None;
    }
    let same = days
        .iter()
        .filter(|d| clocked.is_some_and(|c| c.contains(**d)))
        .count();
    Some(DriverEvidence {
        same: Some(same),
        of: Some(days.len()),
        ..evidence(DriverEvidenceKind::WorkedDays)
    })
}

/// Strong when an email address or phone number only they share agrees with their names,
/// or when the last names agree and either the first name is a short form of the other or
/// the two worked every one of at least three days together; never when they mostly worked
/// apart.
pub(crate) fn strength(evidence: &[DriverEvidence]) -> DriverStrength {
    let has = |kind| evidence.iter().any(|e| e.kind == kind);
    let all_days = evidence.iter().any(|e| {
        e.kind == DriverEvidenceKind::WorkedDays && e.of.unwrap_or(0) >= 3 && e.same == e.of
    });
    let short = has(DriverEvidenceKind::ShortForm);
    let partly = evidence.iter().any(|e| {
        e.kind == DriverEvidenceKind::WorkedDays && e.same.unwrap_or(0) * 2 < e.of.unwrap_or(0)
    });
    let contact = has(DriverEvidenceKind::SameEmail) || has(DriverEvidenceKind::SamePhone);
    let named = [
        DriverEvidenceKind::SameLastName,
        DriverEvidenceKind::SameFirstName,
        DriverEvidenceKind::ShortForm,
        DriverEvidenceKind::LastInitial,
    ]
    .into_iter()
    .any(has);
    if !partly
        && ((contact && named) || (has(DriverEvidenceKind::SameLastName) && (all_days || short)))
    {
        DriverStrength::Strong
    } else {
        DriverStrength::Possible
    }
}

#[cfg(test)]
#[path = "../tests/backend/review.rs"]
mod tests;
