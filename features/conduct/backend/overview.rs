//! What the page reads in one answer: the categories, the people writeups are for, every
//! writeup and where each person stands, each writeup's person as the directory knows them now.
use super::{categories, standing, writeups};
use crate::api::types::{
    ConductDriver, ConductOverview, ConductPerson, ConductWriteup, ConductWriteupRow,
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::Store,
    foundation::clock,
    manifest::people::{self, Everyone},
};
use jiff::civil::Date;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Today where the DSP is.
fn day(c: &Context) -> Result<Date> {
    let zone = dispatch_core::foundation::clock::zone(&c.dsp.timezone)?;
    Ok(clock::today(&zone))
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
    let mut drivers: BTreeMap<String, ConductDriver> = BTreeMap::new();
    let mut category_counts = BTreeMap::new();
    for writeup in &writeups {
        *category_counts
            .entry(writeup.category_id.clone())
            .or_default() += 1;
        let driver = drivers
            .entry(writeup.person.clone())
            .or_insert_with(|| ConductDriver {
                person: writeup.person.clone(),
                count: 0,
                latest_on: writeup.written_on.clone(),
                awaiting_signature: 0,
                categories: Vec::new(),
            });
        driver.count += 1;
        driver.awaiting_signature += usize::from(writeup.signed_on.is_none());
        if !driver.categories.contains(&writeup.category_id) {
            driver.categories.push(writeup.category_id.clone());
        }
    }
    let order: BTreeMap<&str, usize> = categories
        .iter()
        .enumerate()
        .map(|(at, category)| (category.id.as_str(), at))
        .collect();
    for driver in drivers.values_mut() {
        driver
            .categories
            .sort_by_key(|id| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
    }
    let worked = standing::work_out(&categories, &writeups, today);
    Ok(ConductOverview {
        today: today.to_string(),
        standings: worked.standings,
        drivers: drivers.into_values().collect(),
        category_counts,
        categories,
        people,
    })
}

/// The writeup `id` whole, its person by the code the directory knows them by now, as the
/// overview names them.
pub(crate) fn writeup(store: &Store, state: &State, dsp: &str, id: &str) -> Result<ConductWriteup> {
    let mut writeup = writeups::read(store, dsp, id)?;
    let everyone = people::everyone(store, state, dsp)?;
    writeup.row.person = now(&everyone, &writeup.row.person, &writeup.row.person_name).code;
    Members::read(store, dsp, &everyone)?.place(&mut writeup.row);
    Ok(writeup)
}

/// Who the DSP's members are in the directory, for the writeups they wrote or marked signed.
pub(super) struct Members<'a> {
    everyone: &'a Everyone,
    /// Each member's account by their name, for a writeup signed before accounts were kept:
    /// none for a name two members share.
    by_name: HashMap<String, Option<String>>,
}
impl<'a> Members<'a> {
    pub(super) fn read(store: &Store, dsp: &str, everyone: &'a Everyone) -> Result<Self> {
        let mut by_name: HashMap<String, Option<String>> = HashMap::new();
        for member in store.members(dsp)? {
            by_name
                .entry(member.name)
                .and_modify(|account| *account = None)
                .or_insert(Some(member.user_id));
        }
        Ok(Self { everyone, by_name })
    }
    /// Puts to `row` the people its supervisor and whoever marked it signed are.
    pub(super) fn place(&self, row: &mut ConductWriteupRow) {
        let person = |account: &str| {
            self.everyone
                .holding(people::MEMBERS, account)
                .map(|person| person.code.clone())
        };
        row.supervisor = person(&row.accounts.0);
        let by_name = row
            .signed_by_name
            .as_ref()
            .and_then(|name| self.by_name.get(name).cloned().flatten());
        let signer = row.accounts.1.clone().or(by_name);
        row.signed_by = signer.as_deref().and_then(person);
    }
}

/// The person a writeup's `code` names now: whoever kept their IDs once merged, or, once the
/// directory knows nobody by it, the name the writeup printed.
pub(super) fn now(everyone: &Everyone, code: &str, written: &str) -> ConductPerson {
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
