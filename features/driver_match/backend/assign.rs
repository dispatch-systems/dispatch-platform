//! A pass over the DSP's collections: every ID they hold that has no person yet gets one,
//! joining someone only on a certain match.
use super::codes::new_person;
use super::matching::{self, Target};
use super::sources::{self, Identity, Key};
use super::{CHECKED, Collected, IdRow};
use dispatch_core::{
    Result,
    db::{Store, iso},
};
use rusqlite::params;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Reads what a pass needs, without writing: run it under the shared lock.
pub(super) fn read(store: &Store, dsp: &str) -> Result<Collected> {
    let kinds = sources::kinds(store, dsp)?;
    Ok(Collected {
        found: sources::found(&kinds, store, dsp)?,
        contacts_only: sources::contacts_only(&kinds),
    })
}
/// Writes what a pass found: a person for every new ID. Who already holds what is read
/// again here, so a decision made since the read stands.
pub(super) fn write(store: &Store, dsp: &str, collected: Collected) -> Result<usize> {
    let Collected {
        found,
        contacts_only,
    } = collected;
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        let rows: Vec<IdRow> = db.query_as("SELECT * FROM person_ids", [])?;
        let mut holders: BTreeMap<String, Vec<Identity>> = BTreeMap::new();
        for row in &rows {
            let Some(key) = row.key() else { continue };
            let held = match found.get(&key) {
                Some(x) => x.clone(),
                None if contacts_only.contains(&key.0) => continue,
                // An ID no source holds any more has nothing to match by.
                None => Identity::new(key.0, &key.1),
            };
            holders.entry(row.code.clone()).or_default().push(held);
        }
        let known: BTreeSet<Key> = rows.iter().filter_map(IdRow::key).collect();
        let incoming: Vec<Identity> = found
            .into_iter()
            .filter(|(key, _)| !known.contains(key))
            .map(|(_, x)| x)
            .collect();
        let decided = matching::plan(holders, &incoming);
        let now = iso();
        let mut fresh: BTreeMap<usize, String> = BTreeMap::new();
        for decision in &decided {
            let code = match &decision.target {
                Target::Person(code) => code.clone(),
                Target::New(n) => match fresh.get(n) {
                    Some(code) => code.clone(),
                    None => {
                        let code = new_person(store, &db, dsp, &now, None, None)?;
                        fresh.insert(*n, code.clone());
                        code
                    }
                },
            };
            let x = &incoming[decision.index];
            db.exec(
                "INSERT INTO person_ids(source,external_id,code,linked_by,linked_at) \
                 VALUES (?,?,?,?,?)",
                params![x.source, x.id, code, decision.link, now],
            )?;
        }
        db.set(CHECKED, &json!(now))?;
        Ok(decided.len())
    })
}
