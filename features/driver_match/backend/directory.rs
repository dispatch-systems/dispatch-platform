//! Who each ID is, for the features that show people: core's directory, answered from the
//! codes Driver Match keeps.
use super::{IdRow, current_ids, photos};
use dispatch_core::{
    Result, State,
    db::Store,
    manifest::{
        Upkeep,
        people::{self, DriverSource, Person, Photo},
    },
};
use rusqlite::params;
use std::{collections::BTreeMap, sync::Arc};

/// Who each ID is, for the features that show people: Timecard's meal breaks name each
/// driver's Paycom employee by it, and its employees show each one's photo.
pub(crate) struct Directory;
impl people::Directory for Directory {
    fn people(
        &self,
        store: &Store,
        dsp: &str,
        source: DriverSource,
    ) -> Result<BTreeMap<String, Person>> {
        let rows = current_ids(&*store.dsp(dsp)?)?;
        let mut held: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
        for row in &rows {
            held.entry(&row.code).or_default().push(row);
        }
        let versions = photos::versions(store, dsp)?;
        let mut out = BTreeMap::new();
        for (code, ids) in held {
            let mut person = Person {
                code: code.to_owned(),
                confirmed: ids.iter().any(|i| i.linked_by.decided()),
                ..Person::default()
            };
            for id in &ids {
                if let Some(kind) = id.source() {
                    person
                        .ids
                        .entry(kind)
                        .or_default()
                        .push(id.external_id.clone());
                }
            }
            // Their IDs in the sources' order, then as they came, as the tab shows their photo.
            let mut keys: Vec<(&IdRow, _)> =
                ids.iter().filter_map(|i| Some((*i, i.key()?))).collect();
            keys.sort_by_key(|(row, key)| (key.0, row.linked_at.clone()));
            person.photo = keys.iter().find_map(|(_, key)| versions.get(key).cloned());
            for id in person.ids_of(source).to_vec() {
                out.insert(id, person.clone());
            }
        }
        Ok(out)
    }
    fn photo(
        &self,
        store: &Store,
        dsp: &str,
        source: DriverSource,
        id: &str,
    ) -> Result<Option<Photo>> {
        let code = store.dsp(dsp)?.query_as::<(String,)>(
            "SELECT i.code FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL AND i.source=? AND i.external_id=?",
            params![source, id],
        )?;
        match code.into_iter().next() {
            Some((code,)) => photos::photo_of(store, dsp, &code),
            None => Ok(None),
        }
    }
    /// The same pass as after a collection.
    fn changed(&self, state: Arc<State>, dsp: String) -> Upkeep {
        super::maintenance::after_collection(state, dsp)
    }
}
