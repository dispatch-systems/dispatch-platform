//! What the page reads in one answer: the categories, the people writeups are for, every
//! writeup and where each person stands, each writeup's person as the directory knows them now.
use super::{categories, standing, writeups};
use crate::api::types::{ConductOverview, ConductPerson, ConductWriteup};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::Store,
    foundation::clock,
    manifest::people::{self, Everyone},
};
use std::collections::BTreeSet;

/// Today where the DSP is.
fn day(c: &Context) -> Result<NaiveDate> {
    let zone: chrono_tz::Tz = c
        .dsp
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(clock::today(zone))
}
/// Today where the DSP is, YYYY-MM-DD.
pub(crate) fn today(c: &Context) -> Result<String> {
    Ok(day(c)?.to_string())
}

/// Who the person `code` is now, to write a writeup for: refused when the directory doesn't
/// know them.
pub(crate) fn subject(
    store: &Store,
    state: &State,
    dsp: &str,
    code: &str,
) -> Result<writeups::Subject> {
    let everyone = people::everyone(store, state, dsp)?;
    let person = everyone
        .person(code)
        .ok_or_else(|| Error::new("conduct_person_not_found", 404))?;
    Ok(writeups::Subject {
        code: person.code.clone(),
        name: person.name.clone(),
    })
}

pub(crate) fn read(store: &Store, state: &State, c: &Context) -> Result<ConductOverview> {
    let today = day(c)?;
    let everyone = people::everyone(store, state, &c.dsp.id)?;
    let db = store.dsp(&c.dsp.id)?;
    let (categories, mut writeups) =
        db.transaction(|| Ok((categories::all(&db)?, writeups::rows(&db)?)))?;
    let mut people: Vec<ConductPerson> = everyone
        .iter()
        .filter(|person| !person.role.left())
        .map(|person| ConductPerson {
            code: person.code.clone(),
            name: person.name.clone(),
            photo: person.photo.clone(),
            left: false,
        })
        .collect();
    let mut listed: BTreeSet<String> = people.iter().map(|p| p.code.clone()).collect();
    for writeup in &mut writeups {
        let person = now(&everyone, &writeup.person, &writeup.person_name);
        writeup.person.clone_from(&person.code);
        if listed.insert(person.code.clone()) {
            people.push(person);
        }
    }
    people.sort_by(|a, b| {
        dispatch_core::foundation::names::compare(&a.name, &b.name)
            .then_with(|| a.code.cmp(&b.code))
    });
    let worked = standing::work_out(&categories, &writeups, today);
    Ok(ConductOverview {
        today: today.to_string(),
        standings: worked.standings,
        stood_before: worked.stood_before,
        categories,
        people,
        writeups,
    })
}

/// The writeup `id` whole, its person by the code the directory knows them by now, as the
/// overview names them.
pub(crate) fn writeup(store: &Store, state: &State, dsp: &str, id: &str) -> Result<ConductWriteup> {
    let mut writeup = writeups::read(store, dsp, id)?;
    let everyone = people::everyone(store, state, dsp)?;
    writeup.row.person = now(&everyone, &writeup.row.person, &writeup.row.person_name).code;
    Ok(writeup)
}

/// The person a writeup's `code` names now: whoever kept their IDs once merged, or, once the
/// directory knows nobody by it, the name the writeup printed.
fn now(everyone: &Everyone, code: &str, written: &str) -> ConductPerson {
    everyone.person(code).map_or_else(
        || ConductPerson {
            code: code.to_owned(),
            name: written.to_owned(),
            photo: None,
            left: true,
        },
        |person| ConductPerson {
            code: person.code.clone(),
            name: person.name.clone(),
            photo: person.photo.clone(),
            left: person.role.left(),
        },
    )
}
