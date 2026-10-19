//! The names the activity log puts to the codes Driver Match's entries keep: what a
//! collection says about someone stays in its database, so the log reads it as it is shown.
use super::codes::valid_code;
use super::person;
use super::sources::{self, Found};
use super::{IdRow, surviving};
use dispatch_core::{Result, db::Store, manifest::people::Source};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// How an entry names two people by their codes, and the log reads them back.
const AND: &str = " and ";
/// Two people, by their codes, as an entry names them.
pub(super) fn both(a: &str, b: &str) -> String {
    format!("{a}{AND}{b}")
}

/// The name a code's person goes by now, as the page shows it. A merged code answers for the
/// person who kept its IDs.
fn name_now(store: &Store, dsp: &str, found: &Found, code: &str) -> Result<Option<String>> {
    let rows: Vec<IdRow> = {
        let db = store.dsp(dsp)?;
        let code = surviving(&db, code)?;
        db.query_as("SELECT * FROM person_ids WHERE code=?", [&code])?
    };
    let mut ids: Vec<(&IdRow, Source)> = rows
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
        let codes: Vec<&str> = target.split(AND).collect();
        if !action.starts_with("driver_match.") || !codes.iter().all(|c| valid_code(c)) {
            continue;
        }
        let found = described.entry(dsp.to_owned()).or_insert_with(|| {
            sources::kinds(store, dsp)
                .and_then(|kinds| sources::found(&kinds, store, dsp))
                .unwrap_or_default()
        });
        let mut named: Vec<String> = vec![];
        for code in codes {
            let name = known
                .entry((dsp.to_owned(), code.to_owned()))
                .or_insert_with(|| {
                    name_now(store, dsp, found, code)
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
        event["target"] = json!(named.join(AND));
    }
}
