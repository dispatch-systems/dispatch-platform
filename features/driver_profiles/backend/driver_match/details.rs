//! One person in full, for the details panel: what they appear in, their last fourteen days,
//! every decision about them, and their photo from Amazon's roster.
use super::matching::AMAZON;
use super::names::actor_name;
use super::overview::overview;
use super::sources::Key;
use super::{IdRow, surviving, valid_code};
use crate::api::types::{
    DriverActivity, DriverDays, DriverDetails, DriverEvent, DriverEventKind, DriverLink,
};
use crate::backend::storage::DriverProfilesStore;
use dispatch_core::{
    Error, Result,
    db::Store,
    ensure,
    foundation::names,
    manifest::people::{DriverData, DriverSource},
};
use rusqlite::params;
use std::collections::BTreeSet;

/// One person in full: what they appear in, their last fourteen days and every
/// decision about them. A merged person's code opens whoever kept their IDs.
pub(super) fn details(store: &Store, dsp: &str, code: &str) -> Result<DriverDetails> {
    ensure(valid_code(code), "not_found", 404)?;
    let code = surviving(&*store.dsp(dsp)?, code)?;
    let overview = overview(store, dsp)?;
    let driver = overview
        .result
        .drivers
        .iter()
        .find(|d| d.code == code)
        .cloned()
        .ok_or_else(|| Error::new("not_found", 404))?;
    let ids: Vec<(&IdRow, Key)> = overview
        .rows
        .iter()
        .filter(|r| r.code == code)
        .filter_map(|r| Some((r, r.key()?)))
        .collect();
    let activity = DriverData::all()
        .filter_map(|data| {
            let count: usize = ids
                .iter()
                .filter_map(|(_, key)| overview.activity.get(key)?.get(&data))
                .map(|seen| seen.count)
                .sum();
            (count > 0).then_some(DriverActivity { data, count })
        })
        .collect();
    let zone: chrono_tz::Tz = store
        .find_dsp(dsp)?
        .timezone
        .parse()
        .unwrap_or(chrono_tz::UTC);
    let today = chrono::Utc::now().with_timezone(&zone).date_naive();
    let dates: Vec<String> = (0..14)
        .rev()
        .map(|back| (today - chrono::Duration::days(back)).to_string())
        .collect();
    // Each source that knows them and was collected says, for each day it covers,
    // whether they worked.
    let mut known: Vec<DriverSource> = ids.iter().map(|(_, key)| key.0).collect();
    known.sort();
    known.dedup();
    let days = known
        .into_iter()
        .filter_map(|source| {
            let covered = overview.days.collected.get(&source)?;
            let worked: BTreeSet<&String> = ids
                .iter()
                .filter(|(_, key)| key.0 == source)
                .filter_map(|(_, key)| overview.days.of(key))
                .flatten()
                .collect();
            Some(DriverDays {
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
    let db = store.dsp(dsp)?;
    let mut history = vec![];
    for (id, key) in &ids {
        history.push(DriverEvent {
            at: id.linked_at.clone(),
            kind: if id.linked_by == DriverLink::New {
                DriverEventKind::Added
            } else {
                DriverEventKind::Linked
            },
            source: Some(key.0),
            link: Some(id.linked_by),
            name: overview.found.get(key).map(|x| names::display(x.name())),
            code: None,
            actor: actor_name(store, dsp, id.actor_id.as_deref())?,
        });
    }
    let named = |code: &str| {
        overview
            .result
            .drivers
            .iter()
            .find(|d| d.code == code)
            .map(|d| d.name.clone())
    };
    let mut merged_codes = vec![];
    for (merged, at, by) in db.query_as::<(String, Option<String>, Option<String>)>(
        "SELECT code,merged_at,merged_by FROM people WHERE merged_into=? ORDER BY merged_at",
        [&code],
    )? {
        history.push(DriverEvent {
            at: at.unwrap_or_default(),
            kind: DriverEventKind::Merged,
            source: None,
            link: None,
            name: None,
            code: Some(merged.clone()),
            actor: actor_name(store, dsp, by.as_deref())?,
        });
        merged_codes.push(merged);
    }
    for (from, at, by) in db.query_as::<(Option<String>, String, Option<String>)>(
        "SELECT split_from,created_at,created_by FROM people WHERE code=? AND split_from IS NOT NULL",
        [&code],
    )? {
        history.push(DriverEvent {
            at,
            kind: DriverEventKind::Split,
            source: None,
            link: None,
            name: from.as_deref().and_then(named),
            code: from,
            actor: actor_name(store, dsp, by.as_deref())?,
        });
    }
    for (first, second, at, by) in db.query_as::<(String, String, String, Option<String>)>(
        "SELECT first,second,decided_at,actor_id FROM people_apart WHERE first=?1 OR second=?1",
        [&code],
    )? {
        let other = if first == code { second } else { first };
        history.push(DriverEvent {
            at,
            kind: DriverEventKind::Apart,
            source: None,
            link: None,
            name: named(&other),
            code: Some(other),
            actor: actor_name(store, dsp, by.as_deref())?,
        });
    }
    history.sort_by(|a, b| b.at.cmp(&a.at));
    Ok(DriverDetails {
        driver,
        activity,
        dates,
        days,
        history,
        merged_codes,
    })
}

pub(super) fn photo(
    store: &Store,
    dsp: &str,
    code: &str,
) -> Result<Option<(&'static str, Vec<u8>)>> {
    ensure(valid_code(code), "not_found", 404)?;
    let db = store.dsp(dsp)?;
    let code = surviving(&db, code)?;
    let amazon: Vec<String> = db
        .query_as::<(String,)>(
            "SELECT external_id FROM person_ids WHERE code=? AND source=? \
             ORDER BY linked_at,external_id",
            params![code, AMAZON],
        )?
        .into_iter()
        .map(|(id,)| id)
        .collect();
    drop(db);
    store.photo(dsp, &amazon)
}
