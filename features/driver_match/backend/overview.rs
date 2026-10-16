//! What the tab and the details panel read: everyone with a code, as the sources the DSP has
//! on describe them, and the pairs that may be one person, gathered once.
use super::legacy::saved_links;
use super::sources::{self, Days, Identity, Key, Seen};
use super::{CHECKED, FORMER_AFTER_DAYS, IdRow, pairs, person};
use crate::api::types::{Driver, DriverCounts, DriverLink, DriverMatch, DriverPair, DriverRole};
use dispatch_core::{
    Result,
    db::Store,
    foundation::names,
    manifest::people::{DriverData, DriverSource},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Every ID the collections hold now, as they describe it.
pub(crate) type Found = BTreeMap<Key, Identity>;

/// Everything the tab and the details panel read, gathered once.
pub(crate) struct Overview {
    pub result: DriverMatch,
    /// Every ID of the sources on, with its person.
    pub rows: Vec<IdRow>,
    pub found: Found,
    pub activity: BTreeMap<Key, BTreeMap<DriverData, Seen>>,
    pub days: Days,
}

pub(crate) fn overview(store: &Store, dsp: &str) -> Result<Overview> {
    let people = sources::people(store, dsp)?;
    let active = sources::active(&people);
    let found: Found = sources::identities(&people, store, dsp)?
        .into_iter()
        .map(|x| ((x.source, x.id.clone()), x))
        .collect();
    let activity = sources::activity(&people, store, dsp)?;
    let days = sources::days(&people, store, dsp)?;
    let saved = saved_links(store, dsp)?;
    let departments = sources::driver_departments(&people, store, dsp)?;
    // Counted from what was collected, not from today, so a DSP whose collections pause
    // does not see everyone leave.
    let recent = found
        .values()
        .filter_map(|x| x.last_seen.as_deref())
        .chain(
            activity
                .values()
                .flat_map(|data| data.values().filter_map(|s| s.last.as_deref())),
        )
        .filter_map(|day| chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .max()
        .map(|newest| (newest - chrono::Duration::days(FORMER_AFTER_DAYS)).to_string())
        .unwrap_or_default();
    let db = store.dsp(dsp)?;
    // IDs of a source no feature on names sit idle: kept, and shown nowhere.
    let rows: Vec<IdRow> = db
        .query_as::<IdRow>(
            "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.source,i.external_id",
            [],
        )?
        .into_iter()
        .filter(|row| row.source().is_some_and(|source| active.contains(&source)))
        .collect();
    let apart: BTreeSet<(String, String)> = db
        .query_as::<(String, String)>("SELECT first,second FROM people_apart", [])?
        .into_iter()
        .collect();
    let checked_at = db
        .setting(CHECKED, Value::Null)?
        .as_str()
        .map(str::to_owned);
    drop(db);
    // Each person's IDs by their sources' order, then as they came.
    let mut held: BTreeMap<&str, Vec<(&IdRow, DriverSource)>> = BTreeMap::new();
    for row in &rows {
        if let Some(source) = row.source() {
            held.entry(&row.code).or_default().push((row, source));
        }
    }
    for ids in held.values_mut() {
        ids.sort_by_key(|(row, source)| (*source, row.linked_at.clone()));
    }
    let context = person::Context {
        found: &found,
        activity: &activity,
        departments: &departments,
        recent: &recent,
    };
    let mut drivers: BTreeMap<String, Driver> = held
        .iter()
        .map(|(code, ids)| ((*code).to_owned(), person::driver(code, ids, &context)))
        .collect();
    let pairs = pairs::pairs(&held, &drivers, &found, &days, &saved, &apart);
    for pair in &pairs {
        for code in [&pair.person, &pair.candidate] {
            drivers.get_mut(code).unwrap().review = true;
        }
    }
    let review: Vec<DriverPair> = pairs
        .into_iter()
        .map(|pair| DriverPair {
            person: drivers[&pair.person].clone(),
            candidate: drivers[&pair.candidate].clone(),
            strength: pair.strength,
            evidence: pair.evidence,
        })
        .collect();
    let mut list: Vec<Driver> = drivers.into_values().collect();
    list.sort_by(|a, b| names::compare(&a.name, &b.name).then_with(|| a.code.cmp(&b.code)));
    let count = |role: DriverRole| list.iter().filter(|d| d.role == role).count();
    let working = |d: &&Driver| matches!(d.role, DriverRole::Driver | DriverRole::Helper);
    let counts = DriverCounts {
        people: list.len(),
        review: review.len(),
        unconfirmed: list
            .iter()
            .filter(|d| d.role != DriverRole::Former)
            .flat_map(|d| &d.ids)
            .filter(|id| id.linked_by == DriverLink::Variant)
            .count(),
        missing: list
            .iter()
            .filter(working)
            .filter(|d| active.iter().any(|s| d.ids.iter().all(|i| i.source != *s)))
            .count(),
        drivers: count(DriverRole::Driver),
        helpers: count(DriverRole::Helper),
        office: count(DriverRole::Office),
        former: count(DriverRole::Former),
    };
    Ok(Overview {
        result: DriverMatch {
            checked_at,
            sources: active,
            counts,
            review,
            drivers: list,
        },
        rows,
        found,
        activity,
        days,
    })
}
