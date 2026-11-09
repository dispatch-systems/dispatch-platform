//! What the page reads in one answer: the categories, the people writeups are for, every
//! writeup and where each person stands, each writeup's person as the directory knows them now.
use super::{categories, settings, standing, writeups};
use crate::api::types::{ConductOverview, ConductPerson};
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
    let months = settings::general(store, &c.dsp.id)?.months;
    let counts_from = standing::counts_from(today, u32::try_from(months).unwrap_or(12)).to_string();
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
    Ok(ConductOverview {
        standings: standing::standings(&writeups, &counts_from),
        today: today.to_string(),
        counts_from,
        categories,
        people,
        writeups,
    })
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
