//! The names the activity log puts to the codes Driver Match's entries keep: what a
//! collection says about someone stays in its database, so the log reads it as it is shown.
use super::person;
use super::sources::{self, Found};
use super::{IdRow, surviving, valid_code};
use dispatch_core::{Result, db::Store, manifest::people::DriverSource};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The name a code's person goes by now, as the tab shows it. A merged code answers for the
/// person who kept its IDs.
fn driver_name(store: &Store, dsp: &str, found: &Found, code: &str) -> Result<Option<String>> {
    let rows: Vec<IdRow> = {
        let db = store.dsp(dsp)?;
        let code = surviving(&db, code)?;
        db.query_as("SELECT * FROM person_ids WHERE code=?", [&code])?
    };
    let mut ids: Vec<(&IdRow, DriverSource)> = rows
        .iter()
        .filter_map(|row| Some((row, row.source()?)))
        .collect();
    ids.sort_by_key(|(row, source)| (*source, row.linked_at.clone()));
    Ok(person::shown_name(&ids, found))
}
/// Puts today's names to the codes Driver Match's activity entries name. A code that finds
/// no name stays as it is.
pub(crate) fn name_driver_events(store: &Store, events: &mut [Value]) {
    // What the DSP's data says of everyone, read once for each DSP the entries are about.
    let mut described: BTreeMap<String, Found> = BTreeMap::new();
    let mut known: BTreeMap<(String, String), String> = BTreeMap::new();
    for event in events {
        let action = event["action"].as_str().unwrap_or_default();
        let (Some(dsp), Some(target)) = (event["dspId"].as_str(), event["target"].as_str()) else {
            continue;
        };
        let codes: Vec<&str> = target.split(" and ").collect();
        if !action.starts_with("driver_match.") || !codes.iter().all(|c| valid_code(c)) {
            continue;
        }
        let found = described.entry(dsp.to_owned()).or_insert_with(|| {
            sources::people(store, dsp)
                .and_then(|people| sources::found(&people, store, dsp))
                .unwrap_or_default()
        });
        let mut named: Vec<String> = vec![];
        for code in codes {
            let name = known
                .entry((dsp.to_owned(), code.to_owned()))
                .or_insert_with(|| {
                    driver_name(store, dsp, found, code)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| code.to_owned())
                })
                .clone();
            // Two codes merged into one person are that one person now.
            if action != "driver_match.merged" || !named.contains(&name) {
                named.push(name);
            }
        }
        event["target"] = json!(named.join(" and "));
    }
}

/// Who did something, as the DSP sees them.
pub(super) fn actor_name(store: &Store, dsp: &str, id: Option<&str>) -> Result<Option<String>> {
    let Some(id) = id else { return Ok(None) };
    store.actor_name(dsp, id)
}
