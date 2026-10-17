//! A pass over the DSP's collections: every ID they hold that has no person yet gets one,
//! joining someone only on a certain match.
use super::legacy::saved_links;
use super::matching::{self, Member};
use super::sources::{self, Identity, Key};
use super::{ALPHABET, CHECKED, CODE_LENGTH, DriverSources, IdRow};
use dispatch_core::{
    Error, Result,
    db::{Db, Store, iso},
    foundation::crypto,
};
use rusqlite::params;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Reads what a pass needs, without writing: run it under the shared lock.
pub(super) fn read(store: &Store, dsp: &str) -> Result<DriverSources> {
    let people = sources::people(store, dsp)?;
    Ok(DriverSources {
        identities: sources::identities(&people, store, dsp)?,
        saved: saved_links(store, dsp)?,
        contacts_only: sources::contacts_only(&people),
    })
}
/// Writes what a pass found: a person for every new ID. Who already holds what is read
/// again here, so a decision made since the read stands.
pub(super) fn write(store: &Store, dsp: &str, found: DriverSources) -> Result<usize> {
    let DriverSources {
        identities,
        saved,
        contacts_only,
    } = found;
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        let rows: Vec<IdRow> = db.query_as("SELECT * FROM person_ids", [])?;
        let known: BTreeSet<(String, String)> = rows
            .iter()
            .map(|r| (r.source.clone(), r.external_id.clone()))
            .collect();
        let described: BTreeMap<Key, &Identity> = identities
            .iter()
            .map(|x| ((x.source, x.id.clone()), x))
            .collect();
        let incoming: Vec<Identity> = identities
            .iter()
            .filter(|x| !known.contains(&(x.source.as_str().to_owned(), x.id.clone())))
            .cloned()
            .collect();
        let mut people: BTreeMap<String, Vec<Member>> = BTreeMap::new();
        for row in &rows {
            let Some(key) = row.key() else { continue };
            let member = match described.get(&key) {
                Some(x) => Member::of(x),
                None if contacts_only.contains(&key.0) => continue,
                None => Member::gone(key),
            };
            people.entry(row.code.clone()).or_default().push(member);
        }
        let decided = matching::plan(people, &incoming, &saved);
        let now = iso();
        let mut fresh: BTreeMap<String, String> = BTreeMap::new();
        for (index, person, link) in &decided {
            let code = if !person.starts_with('+') {
                person.clone()
            } else if let Some(code) = fresh.get(person) {
                code.clone()
            } else {
                let code = new_person(store, &db, dsp, &now, None, None)?;
                fresh.insert(person.clone(), code.clone());
                code
            };
            let x = &incoming[*index];
            db.exec(
                "INSERT INTO person_ids(source,external_id,code,linked_by,linked_at) \
                 VALUES (?,?,?,?,?)",
                params![x.source, x.id, code, link, now],
            )?;
        }
        db.set(CHECKED, &json!(now))?;
        Ok(decided.len())
    })
}

/// A person with a new code, unique on the platform: every DSP's codes are listed in
/// `driver_codes`, so a code never leads to two people anywhere.
pub(super) fn new_person(
    store: &Store,
    db: &Db,
    dsp: &str,
    now: &str,
    split_from: Option<&str>,
    actor: Option<&str>,
) -> Result<String> {
    for _ in 0..16 {
        // 240 is the largest multiple of 30 a byte holds; larger bytes would favour
        // the alphabet's first letters, so they are skipped.
        let code: String = crypto::random::<24>()?
            .into_iter()
            .filter(|b| *b < 240)
            .take(CODE_LENGTH)
            .map(|b| ALPHABET[usize::from(b % 30)] as char)
            .collect();
        if code.len() == CODE_LENGTH
            && store.platform.exec(
                "INSERT OR IGNORE INTO driver_codes(code,dsp_id,created_at) VALUES (?,?,?)",
                params![code, dsp, now],
            )? == 1
        {
            db.exec(
                "INSERT INTO people(code,created_at,split_from,created_by) VALUES (?,?,?,?)",
                params![code, now, split_from, actor],
            )?;
            return Ok(code);
        }
    }
    Err(Error::new("driver_code_unavailable", 500))
}
