//! What the page, a person's sheet and the count read from the DSP's data, gathered once for
//! each answer: what every source says of each ID, which IDs count, and each person's IDs.
use super::person::{self, Context};
use super::sources::{self, Days, DriverDepartments, Found, Key, Seen};
use super::{FORMER_AFTER_DAYS, IdRow, current_ids};
use crate::api::types::Person;
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{Kind, Source},
};
use std::collections::{BTreeMap, BTreeSet};

/// Everything read for one answer.
pub(super) struct Gathered {
    pub found: Found,
    pub activity: BTreeMap<Key, BTreeMap<Kind, Seen>>,
    pub days: Days,
    departments: DriverDepartments,
    driving: BTreeSet<Kind>,
    /// The first day that still counts as recent.
    recent: String,
    /// The DSP's systems, in their order.
    pub sources: Vec<Source>,
    /// Every ID that counts, of the DSP's systems, as they came.
    pub rows: Vec<IdRow>,
}

/// Reads what every kind of data the DSP has on says, and which of the IDs people hold count:
/// a source no feature on names sits idle, as does an ID a list of people the DSP can reach no
/// longer names; both are kept, and shown nowhere.
pub(super) fn gather(store: &Store, dsp: &str) -> Result<Gathered> {
    let kinds = sources::kinds(store, dsp)?;
    let found = sources::found(&kinds, store, dsp)?;
    let activity = sources::activity(&kinds, store, dsp)?;
    let days = sources::days(&kinds, store, dsp)?;
    let departments = sources::driver_departments(&kinds, store, dsp)?;
    let driving = sources::driving(&kinds);
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
    let contacts_only = sources::contacts_only(&kinds);
    let every: Vec<IdRow> = current_ids(&*store.dsp(dsp)?)?
        .into_iter()
        .filter(|row| {
            row.key()
                .is_none_or(|key| !contacts_only.contains(&key.0) || found.contains_key(&key))
        })
        .collect();
    let sources = sources::active(&kinds, &found, every.iter().filter_map(IdRow::source));
    let rows = every
        .into_iter()
        .filter(|row| row.source().is_some_and(|source| sources.contains(&source)))
        .collect();
    Ok(Gathered {
        found,
        activity,
        days,
        departments,
        driving,
        recent,
        sources,
        rows,
    })
}

impl Gathered {
    /// Each person's IDs, by their code, in their sources' order, then as they came.
    pub fn held(&self) -> BTreeMap<&str, Vec<(&IdRow, Source)>> {
        let mut held: BTreeMap<&str, Vec<(&IdRow, Source)>> = BTreeMap::new();
        for row in &self.rows {
            if let Some(source) = row.source() {
                held.entry(&row.code).or_default().push((row, source));
            }
        }
        for ids in held.values_mut() {
            ids.sort_by_key(|(row, source)| (*source, row.linked_at.clone()));
        }
        held
    }
    /// One person as the page lists them, from their IDs.
    pub fn person(&self, code: &str, ids: &[(&IdRow, Source)]) -> Person {
        let context = Context {
            found: &self.found,
            activity: &self.activity,
            departments: &self.departments,
            driving: &self.driving,
            recent: &self.recent,
        };
        person::person(code, ids, &context)
    }
}
