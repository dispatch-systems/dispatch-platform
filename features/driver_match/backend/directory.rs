//! Who each ID is, for the features that show people: core's directory, answered from the
//! codes Driver Match keeps.
use super::{IdRow, details, sources};
use crate::api::types::DriverLink;
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
        let rows: Vec<IdRow> = store.dsp(dsp)?.query_as(
            "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.external_id",
            [],
        )?;
        let mut held: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
        for row in &rows {
            held.entry(&row.code).or_default().push(row);
        }
        let photos = photos(store, dsp)?;
        let mut out = BTreeMap::new();
        for (code, ids) in held {
            let mut person = Person {
                code: code.to_owned(),
                confirmed: ids
                    .iter()
                    .any(|i| matches!(i.linked_by, DriverLink::Person | DriverLink::Saved)),
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
            person.photo = photos.iter().find_map(|(source, versions)| {
                person
                    .ids_of(*source)
                    .iter()
                    .find_map(|id| versions.get(id).cloned())
            });
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
            Some((code,)) => details::photo(store, dsp, &code),
            None => Ok(None),
        }
    }
    /// The same pass as after a collection.
    fn changed(&self, state: Arc<State>, dsp: String) -> Upkeep {
        super::maintenance::after_collection(state, dsp)
    }
}

/// The version of each photo the kinds of data that hold photos name, by ID, in the order
/// `details::photo` looks through them: the version of the photo it serves.
fn photos(store: &Store, dsp: &str) -> Result<Vec<(DriverSource, BTreeMap<String, String>)>> {
    let mut out = vec![];
    for people in sources::people(store, dsp)? {
        if people.photos.is_none() {
            continue;
        }
        let versions = (people.read)(store, dsp)?
            .into_iter()
            .filter_map(|named| Some((named.id, named.photo?)))
            .collect();
        out.push((people.source(), versions));
    }
    Ok(out)
}
