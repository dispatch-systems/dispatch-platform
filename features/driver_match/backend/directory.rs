//! Who each ID is, for the features that show people: core's directory, answered from the
//! codes Driver Match keeps.
use super::IdRow;
use crate::api::types::DriverLink;
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{self, DriverSource, Link},
};
use std::collections::BTreeMap;

/// Who each ID is, for the features that show people: Timecard's meal breaks name each
/// driver's Paycom employee by it.
pub(crate) struct Directory;
impl people::Directory for Directory {
    fn links(
        &self,
        store: &Store,
        dsp: &str,
        from: DriverSource,
        to: DriverSource,
    ) -> Result<BTreeMap<String, Link>> {
        let rows: Vec<IdRow> = store.dsp(dsp)?.query_as(
            "SELECT i.* FROM person_ids i JOIN people p ON p.code=i.code \
             WHERE p.merged_into IS NULL ORDER BY i.linked_at,i.external_id",
            [],
        )?;
        let mut people: BTreeMap<&str, Vec<&IdRow>> = BTreeMap::new();
        for row in &rows {
            people.entry(&row.code).or_default().push(row);
        }
        let mut out = BTreeMap::new();
        for (code, ids) in people {
            let link = Link {
                code: code.to_owned(),
                ids: ids
                    .iter()
                    .filter(|i| i.source == to.as_str())
                    .map(|i| i.external_id.clone())
                    .collect(),
                confirmed: ids
                    .iter()
                    .any(|i| matches!(i.linked_by, DriverLink::Person | DriverLink::Saved)),
            };
            for id in ids.iter().filter(|i| i.source == from.as_str()) {
                out.insert(id.external_id.clone(), link.clone());
            }
        }
        Ok(out)
    }
}
