//! Who each ID is, for the features that show people: core's directory, answered from the
//! codes Driver Match keeps.
use super::IdRow;
use crate::api::types::DriverLink;
use dispatch_core::{
    Result, State,
    db::Store,
    manifest::{
        Upkeep,
        people::{self, DriverSource, Person},
    },
};
use std::{collections::BTreeMap, sync::Arc};

/// Who each ID is, for the features that show people: Timecard's meal breaks name each
/// driver's Paycom employee by it.
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
            for id in person.ids_of(source).to_vec() {
                out.insert(id, person.clone());
            }
        }
        Ok(out)
    }
    /// The same pass as after a collection.
    fn changed(&self, state: Arc<State>, dsp: String) -> Upkeep {
        super::maintenance::after_collection(state, dsp)
    }
}
