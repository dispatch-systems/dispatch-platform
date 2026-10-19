//! What the page and its count read: everyone with a code, as the sources the DSP has on
//! describe them, and the questions waiting for a person.
use super::gather::{Gathered, gather};
use super::{CHECKED, IdRow, pairs};
use crate::api::types::{DriverMatch, IdLink, MatchCounts, Person, Question};
use dispatch_core::{Result, db::Store, foundation::names, manifest::people::Source};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The Driver Match page: the questions waiting, then every person with a code, by name.
pub(super) fn driver_match(store: &Store, dsp: &str) -> Result<DriverMatch> {
    let gathered = gather(store, dsp)?;
    let held = gathered.held();
    let people = people(&gathered, &held);
    let questions = questions(store, dsp, &gathered, &held, &people)?;
    let checked_at = store
        .dsp(dsp)?
        .setting(CHECKED, Value::Null)?
        .as_str()
        .map(str::to_owned);
    let mut people: Vec<Person> = people.into_values().collect();
    people.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
    Ok(DriverMatch {
        checked_at,
        sources: gathered.sources,
        questions,
        people,
    })
}

/// How many questions wait, for the sidebar's count.
pub(super) fn counts(store: &Store, dsp: &str) -> Result<MatchCounts> {
    let gathered = gather(store, dsp)?;
    let held = gathered.held();
    let people = people(&gathered, &held);
    Ok(MatchCounts {
        questions: questions(store, dsp, &gathered, &held, &people)?.len(),
    })
}

/// Everyone with a code, by it.
fn people(
    gathered: &Gathered,
    held: &BTreeMap<&str, Vec<(&IdRow, Source)>>,
) -> BTreeMap<String, Person> {
    held.iter()
        .map(|(code, ids)| ((*code).to_owned(), gathered.person(code, ids)))
        .collect()
}

/// The questions waiting: the pairs that may be one person, the strongest first, then the
/// IDs a name written another way joined to someone, which nobody has confirmed. Nobody is
/// asked about someone who has left.
fn questions(
    store: &Store,
    dsp: &str,
    gathered: &Gathered,
    held: &BTreeMap<&str, Vec<(&IdRow, Source)>>,
    people: &BTreeMap<String, Person>,
) -> Result<Vec<Question>> {
    let apart: BTreeSet<(String, String)> = store
        .dsp(dsp)?
        .query_as::<(String, String)>("SELECT first,second FROM people_apart", [])?
        .into_iter()
        .collect();
    let pairs = pairs::pairs(held, people, &gathered.found, &gathered.days, &apart);
    let mut asked: Vec<&Person> = people.values().filter(|p| !p.role.left()).collect();
    asked.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
    let variants = asked.into_iter().flat_map(|person| {
        person
            .ids
            .iter()
            .filter(|id| id.linked_by == IdLink::Variant)
            .map(|id| Question::Variant {
                person: person.code.clone(),
                source: id.source,
                id: id.id.clone(),
            })
    });
    Ok(pairs
        .into_iter()
        .map(|pair| Question::Pair {
            person: pair.person,
            candidate: pair.candidate,
            strength: pair.strength,
            evidence: pair.evidence,
        })
        .chain(variants)
        .collect())
}
