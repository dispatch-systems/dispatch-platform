//! Who might be one person listed twice: two people no source knows both of, whose names
//! come close without matching, or any two who alone share an email address or phone
//! number. A person decides; this only gathers the reasons.
use super::contacts::Contact;
use super::names::{Name, capitalized, short_form};
use crate::api::types::{Evidence, EvidenceKind, Strength};
use dispatch_core::manifest::people::Source;
use std::collections::{BTreeMap, BTreeSet};

/// How many days two people must have worked together, every one, for their names alone to
/// make them a strong pair.
const SURE_AFTER_DAYS: usize = 3;

/// One person who might be another: their code, the sources whose knowing both would keep
/// two people apart, and every name and contact their IDs carry.
pub(crate) struct Side<'a> {
    pub code: &'a str,
    pub sources: BTreeSet<Source>,
    pub names: Vec<&'a str>,
    pub contacts: BTreeSet<(Contact, &'a str)>,
    /// Whether they're asked about: someone who has left counts among those carrying a
    /// contact, but is never asked about.
    pub asked: bool,
}
pub(crate) struct Suggestion {
    pub first: String,
    pub second: String,
    pub evidence: Vec<Evidence>,
}

/// A reason of `kind` that argues for the two being one.
fn evidence(kind: EvidenceKind) -> Evidence {
    Evidence {
        kind,
        short: None,
        long: None,
        same: None,
        of: None,
        supports: true,
    }
}

/// The reasons two names might be one person's, or none when nothing ties them: a shared
/// last name, or a first name and a last initial.
fn reasons(a: &Name, b: &Name) -> Vec<Evidence> {
    let (Some(last_a), Some(last_b)) = (a.last(), b.last()) else {
        return vec![];
    };
    let same_last = a.same_last(b);
    // A source may write only a last initial, as "Jordan K."
    let initial = !same_last
        && (last_a.chars().count() == 1 || last_b.chars().count() == 1)
        && last_a.chars().next() == last_b.chars().next();
    let first = if a.same_given(b) {
        Some(evidence(EvidenceKind::SameFirstName))
    } else {
        short_form(&a.given, &b.given).map(|(short, long)| Evidence {
            short: Some(capitalized(short)),
            long: Some(capitalized(long)),
            ..evidence(EvidenceKind::ShortForm)
        })
    };
    let tied = same_last || (initial && first.is_some());
    if !tied {
        return vec![];
    }
    let mut out = vec![evidence(if same_last {
        EvidenceKind::SameLastName
    } else {
        EvidenceKind::LastInitial
    })];
    out.extend(first);
    out
}

/// Every pair worth a look, with the contacts only the two of them share and the strongest
/// reasons their names give. Two people one source knows both of are a pair only by a
/// shared contact, as a rehire under a second ID is. `apart` holds the pairs someone already
/// decided are different people, as (first, second) codes.
pub(crate) fn suggest(sides: &[Side<'_>], apart: &BTreeSet<(String, String)>) -> Vec<Suggestion> {
    // How many people carry each contact: one that more than two carry says nothing.
    let mut carried: BTreeMap<(Contact, &str), usize> = BTreeMap::new();
    for side in sides {
        for contact in &side.contacts {
            *carried.entry(*contact).or_default() += 1;
        }
    }
    let names: Vec<Vec<Name>> = sides
        .iter()
        .map(|side| side.names.iter().map(|n| Name::new(n)).collect())
        .collect();
    let mut out = vec![];
    for (i, a) in sides.iter().enumerate().filter(|(_, side)| side.asked) {
        for (j, b) in sides
            .iter()
            .enumerate()
            .skip(i + 1)
            .filter(|(_, side)| side.asked)
        {
            let key = if a.code < b.code {
                (a.code.to_owned(), b.code.to_owned())
            } else {
                (b.code.to_owned(), a.code.to_owned())
            };
            if apart.contains(&key) {
                continue;
            }
            let mut found: Vec<Evidence> = Contact::ALL
                .into_iter()
                .filter(|kind| {
                    a.contacts
                        .intersection(&b.contacts)
                        .any(|shared| shared.0 == *kind && carried.get(shared) == Some(&2))
                })
                .map(|kind| evidence(kind.evidence()))
                .collect();
            if found.is_empty() && !a.sources.is_disjoint(&b.sources) {
                continue;
            }
            let best = names[i]
                .iter()
                .flat_map(|x| names[j].iter().map(move |y| reasons(x, y)))
                .max_by_key(|r| {
                    (
                        r.len(),
                        r.iter().any(|e| e.kind == EvidenceKind::SameLastName),
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

/// The days one person worked, where both people's sources were collected, and how many of
/// them the other worked too. `None` with fewer than two such days, which say nothing.
pub(crate) fn worked_days(
    one: &BTreeSet<String>,
    other: &BTreeSet<String>,
    one_collected: &BTreeSet<String>,
    other_collected: &BTreeSet<String>,
) -> Option<Evidence> {
    let window: BTreeSet<&String> = one_collected.intersection(other_collected).collect();
    let days: Vec<&String> = one.iter().filter(|d| window.contains(d)).collect();
    if days.len() < 2 {
        return None;
    }
    let same = days.iter().filter(|d| other.contains(**d)).count();
    // Working fewer than half of them together argues against.
    Some(Evidence {
        same: Some(same),
        of: Some(days.len()),
        supports: same * 2 >= days.len(),
        ..evidence(EvidenceKind::WorkedDays)
    })
}

/// Strong when a contact only they share agrees with their names, or when the last names
/// agree and either the first name is a short form of the other or the two worked every one
/// of at least `SURE_AFTER_DAYS` days together; never when they worked fewer than half their
/// days together.
pub(crate) fn strength(evidence: &[Evidence]) -> Strength {
    let has = |kind| evidence.iter().any(|e| e.kind == kind);
    let worked = evidence
        .iter()
        .find(|e| e.kind == EvidenceKind::WorkedDays)
        .map(|e| (e.same.unwrap_or(0), e.of.unwrap_or(0)));
    let all_days = worked.is_some_and(|(same, of)| of >= SURE_AFTER_DAYS && same == of);
    let partly = evidence.iter().any(|e| !e.supports);
    let short = has(EvidenceKind::ShortForm);
    let contact = Contact::ALL.into_iter().any(|kind| has(kind.evidence()));
    let named = [
        EvidenceKind::SameLastName,
        EvidenceKind::SameFirstName,
        EvidenceKind::ShortForm,
        EvidenceKind::LastInitial,
    ]
    .into_iter()
    .any(has);
    if !partly && ((contact && named) || (has(EvidenceKind::SameLastName) && (all_days || short))) {
        Strength::Strong
    } else {
        Strength::Possible
    }
}

#[cfg(test)]
#[path = "../tests/backend/review.rs"]
mod tests;
