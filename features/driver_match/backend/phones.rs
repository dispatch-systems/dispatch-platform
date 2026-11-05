//! The phone numbers someone at the DSP adds to a person, beside those their sources have, as a
//! driver's new or second phone: the directory gives them to the person, so every feature knows
//! who texts from one, and they join no one to anyone. Anyone who can text adds them, through
//! core's `people::add_phone`, and takes them off again; a manager does too, on the person's
//! sheet. Each is audited by the person's code, with the number.
use super::codes::valid_code;
use super::decisions::{audited, current};
use super::directory::Directory;
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::{Db, Store, iso},
    ensure,
    manifest::people::{Directory as _, same_phone},
};
use rusqlite::params;
use std::collections::BTreeMap;

/// The most characters an added number may be written with.
const NUMBER_MOST: usize = 32;

/// The numbers someone added to each person, by their code, the first added first.
pub(super) fn added(db: &Db) -> Result<BTreeMap<String, Vec<String>>> {
    let mut added: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (code, phone) in db.query_as::<(String, String)>(
        "SELECT code,phone FROM person_phones ORDER BY added_at,phone",
        [],
    )? {
        added.entry(code).or_default().push(phone);
    }
    Ok(added)
}

/// The numbers someone added to `code`, the first added first.
pub(super) fn added_to(store: &Store, dsp: &str, code: &str) -> Result<Vec<String>> {
    Ok(store
        .dsp(dsp)?
        .query_as::<(String,)>(
            "SELECT phone FROM person_phones WHERE code=? ORDER BY added_at,phone",
            [code],
        )?
        .into_iter()
        .map(|(phone,)| phone)
        .collect())
}

/// Adds `number` to the person `code`'s, as the member `c` decided. Refused while anyone has it,
/// from a source or added, however it's written (`driver_match_phone_taken`), and for anyone
/// who no longer holds their IDs (`person_changed`).
pub(super) fn add(store: &Store, c: &Context, code: &str, number: &str) -> Result<()> {
    let number = number.trim();
    ensure(
        valid_code(code)
            && !number.is_empty()
            && number.len() <= NUMBER_MOST
            && number.chars().filter(char::is_ascii_digit).count() >= 7,
        "invalid_input",
        400,
    )?;
    let dsp = c.dsp.id.as_str();
    ensure(
        Directory.everyone(store, dsp)?.by_phone(number).is_empty(),
        "driver_match_phone_taken",
        409,
    )?;
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        current(&db, code)?;
        db.exec(
            "INSERT INTO person_phones(phone,code,added_at,actor_id) VALUES (?,?,?,?)",
            params![number, code, iso(), c.actor()],
        )?;
        Ok(())
    })?;
    drop(db);
    audited(
        store,
        c,
        "driver_match.phone_added",
        code,
        code,
        &[("phone", None, Some(number.to_owned()))],
        code,
    )
}

/// Takes `number` off the person `code`, whom someone added it to, however it's written, as the
/// member `c` decided: `not_found` when no one has it added, and `person_changed` once it's
/// someone else's, as a sheet or conversation read before it moved shows it as `code`'s. A merged
/// code answers for whoever kept their IDs.
pub(super) fn remove(store: &Store, c: &Context, code: &str, number: &str) -> Result<()> {
    ensure(valid_code(code), "invalid_input", 400)?;
    let dsp = c.dsp.id.as_str();
    let everyone = Directory.everyone(store, dsp)?;
    let person = everyone
        .added(number)
        .ok_or_else(|| Error::new("not_found", 404))?;
    ensure(
        everyone
            .person(code)
            .is_some_and(|chosen| chosen.code == person.code),
        "person_changed",
        409,
    )?;
    let code = person.code.clone();
    // The number as it was added, which `added` found it by.
    let removed: Vec<String> = person
        .added_phones
        .iter()
        .filter(|added| same_phone(added, number))
        .cloned()
        .collect();
    let db = store.dsp(dsp)?;
    db.transaction(|| {
        for phone in &removed {
            db.exec(
                "DELETE FROM person_phones WHERE phone=? AND code=?",
                params![phone, code],
            )?;
        }
        Ok(())
    })?;
    drop(db);
    audited(
        store,
        c,
        "driver_match.phone_removed",
        &code,
        &code,
        &[("phone", removed.first().cloned(), None)],
        &code,
    )
}
