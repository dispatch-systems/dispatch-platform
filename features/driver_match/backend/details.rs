//! One person in full, for their sheet: what they appear in, their last fourteen days and
//! every decision about them.
use super::codes::valid_code;
use super::gather::gather;
use super::person::shown_name;
use super::sources::Key;
use super::{IdRow, surviving};
use crate::api::types::{
    Appearance, IdLink, PersonDetails, PersonEvent, PersonEventKind, SourceDays,
};
use dispatch_core::{
    Error, Result,
    db::Store,
    ensure,
    foundation::names,
    manifest::people::{Kind, Source},
};
use std::collections::{BTreeMap, BTreeSet};

/// How many days a person's sheet shows of their work, ending today.
const DAYS: i64 = 14;

/// One person in full. A merged person's code opens whoever kept their IDs.
pub(super) fn details(store: &Store, dsp: &str, code: &str) -> Result<PersonDetails> {
    ensure(valid_code(code), "not_found", 404)?;
    let code = surviving(&*store.dsp(dsp)?, code)?;
    let gathered = gather(store, dsp)?;
    let held = gathered.held();
    let ids = held
        .get(code.as_str())
        .ok_or_else(|| Error::new("not_found", 404))?;
    let person = gathered.person(&code, ids);
    let keys: Vec<(&IdRow, Key)> = ids
        .iter()
        .map(|(row, source)| (*row, (*source, row.external_id.clone())))
        .collect();
    let appears = Kind::all()
        .filter_map(|data| {
            let count: usize = keys
                .iter()
                .filter_map(|(_, key)| gathered.activity.get(key)?.get(&data))
                .map(|seen| seen.count)
                .sum();
            (count > 0).then_some(Appearance { data, count })
        })
        .collect();
    let dates = last_days(store, dsp)?;
    // Each source that knows them and was collected says, for each day it covers, whether
    // they worked.
    let known: BTreeSet<Source> = keys.iter().map(|(_, key)| key.0).collect();
    let days = known
        .into_iter()
        .filter_map(|source| {
            let covered = gathered.days.collected.get(&source)?;
            let worked: BTreeSet<&String> = keys
                .iter()
                .filter(|(_, key)| key.0 == source)
                .filter_map(|(_, key)| gathered.days.of(key))
                .flatten()
                .collect();
            Some(SourceDays {
                source,
                worked: dates
                    .iter()
                    .map(|date| {
                        let here = worked.contains(date);
                        (here || covered.contains(date)).then_some(here)
                    })
                    .collect(),
            })
        })
        .collect();
    let named = |other: &str| {
        held.get(other)
            .and_then(|ids| shown_name(ids, &gathered.found))
    };
    let history = history(store, dsp, &code, &keys, &gathered.found, named)?;
    Ok(PersonDetails {
        person,
        appears,
        dates,
        days,
        history,
    })
}

/// The last `DAYS` days, oldest first, in the DSP's time zone.
fn last_days(store: &Store, dsp: &str) -> Result<Vec<String>> {
    let zone: chrono_tz::Tz = store
        .find_dsp(dsp)?
        .timezone
        .parse()
        .unwrap_or(chrono_tz::UTC);
    let today = dispatch_core::foundation::clock::today(zone);
    Ok((0..DAYS)
        .rev()
        .map(|back| (today - chrono::Duration::days(back)).to_string())
        .collect())
}

/// Every line of a person's history, newest first: each ID as it came to them, and each
/// decision about them and another person, with who made it.
fn history(
    store: &Store,
    dsp: &str,
    code: &str,
    keys: &[(&IdRow, Key)],
    found: &super::sources::Found,
    named: impl Fn(&str) -> Option<String>,
) -> Result<Vec<PersonEvent>> {
    let db = store.dsp(dsp)?;
    let mut actors: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut actor = |id: Option<&str>| -> Result<Option<String>> {
        let Some(id) = id else { return Ok(None) };
        if !actors.contains_key(id) {
            actors.insert(id.to_owned(), store.actor_name(dsp, id)?);
        }
        Ok(actors[id].clone())
    };
    let event = |at: String, kind: PersonEventKind| PersonEvent {
        at,
        kind,
        source: None,
        link: None,
        name: None,
        code: None,
        actor: None,
    };
    let mut history = vec![];
    for (row, key) in keys {
        history.push(PersonEvent {
            source: Some(key.0),
            link: Some(row.linked_by),
            name: found.get(key).map(|x| names::display(x.name())),
            actor: actor(row.actor_id.as_deref())?,
            ..event(
                row.linked_at.clone(),
                if row.linked_by == IdLink::New {
                    PersonEventKind::Added
                } else {
                    PersonEventKind::Linked
                },
            )
        });
    }
    for (merged, at, by) in db.query_as::<(String, Option<String>, Option<String>)>(
        "SELECT code,merged_at,merged_by FROM people WHERE merged_into=? ORDER BY merged_at",
        [code],
    )? {
        history.push(PersonEvent {
            code: Some(merged),
            actor: actor(by.as_deref())?,
            ..event(at.unwrap_or_default(), PersonEventKind::Merged)
        });
    }
    for (from, at, by) in db.query_as::<(Option<String>, String, Option<String>)>(
        "SELECT split_from,created_at,created_by FROM people WHERE code=? AND split_from IS NOT NULL",
        [code],
    )? {
        history.push(PersonEvent {
            name: from.as_deref().and_then(&named),
            code: from,
            actor: actor(by.as_deref())?,
            ..event(at, PersonEventKind::Split)
        });
    }
    for (first, second, at, by) in db.query_as::<(String, String, String, Option<String>)>(
        "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
        [code],
    )? {
        let other = if first == code { second } else { first };
        history.push(PersonEvent {
            name: named(&other),
            code: Some(other),
            actor: actor(by.as_deref())?,
            ..event(at, PersonEventKind::Apart)
        });
    }
    history.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(history)
}
