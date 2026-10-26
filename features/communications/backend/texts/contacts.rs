//! Whom the DSP can text, and what someone at the DSP does with a number. Messaging's Contacts
//! lists everyone Driver Match has a phone number for who hasn't left, by their first, and every
//! number the DSP saved a name for, each number once, by name, with its conversation if it has
//! one; then the numbers blocked. A name typed, misspelt or not, finds them as it finds
//! conversations; digits typed, by their number. A number's conversation, or one of today's
//! drivers', is opened to text it first, and a number saved under a name, shared at the DSP, or
//! blocked, or unblocked.
use super::{
    live_day::LiveDay,
    numbers::Numbers,
    offered, offering,
    reading::{like, numbered, one},
    storage::{Filed, TextStore},
};
use crate::{
    api::types::{Conversation, TextContact, TextContacts},
    backend::{conversations::People, live},
};
use dispatch_core::{Error, Result, State, db::Store, ensure, foundation::names};
use dispatch_phones_contract as texting;
use std::collections::{HashMap, HashSet};

/// How many are listed with nothing typed, at most.
const LISTED: usize = 2000;
/// How many are found with something typed, at most.
const FOUND: usize = 20;
/// The most a contact's name may be, in characters.
pub const NAME_MOST: usize = 80;

/// Whom the DSP can text, all of them or those like `typed`, and the numbers blocked.
pub fn contacts(db: &Store, state: &State, dsp: &str, typed: Option<&str>) -> Result<TextContacts> {
    if !offered(db, dsp)? {
        return Ok(TextContacts {
            contacts: Vec::new(),
            blocked: Vec::new(),
        });
    }
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let conversations: HashMap<String, (String, Option<i64>, Option<i64>)> = db
        .communications_numbers(dsp)?
        .into_iter()
        .filter_map(|each| Some((each.number?, (each.id, each.waiting_since, each.last_at))))
        .collect();
    let contact = |number: &str, matched: Vec<(u32, u32)>| {
        let conversation = conversations.get(number);
        TextContact {
            number: number.to_owned(),
            person: people.texting(number, numbers.contact(number)),
            conversation_id: conversation.map(|(id, _, _)| id.clone()),
            waiting_since: conversation.and_then(|(_, since, _)| *since),
            last_at: conversation.and_then(|(_, _, last)| *last),
            matched,
        }
    };
    // Each number once: people's first, then those the DSP named.
    let mut listed = Vec::new();
    let mut seen = HashSet::new();
    let theirs = people
        .everyone()
        .iter()
        .filter(|person| !person.role.left())
        .filter_map(|person| {
            person
                .phones
                .iter()
                .find_map(|phone| texting::normal(phone))
        });
    for number in theirs.chain(numbers.contacts().map(str::to_owned)) {
        if !numbers.blocked(&number) && seen.insert(number.clone()) {
            listed.push(number);
        }
    }
    let mut found: Vec<(f64, TextContact)> = listed
        .iter()
        .filter_map(|number| {
            let listed = contact(number, Vec::new());
            let (score, matched) = match typed {
                Some(typed) => like(typed, &listed.person.name, std::slice::from_ref(number))?,
                None => (0.0, Vec::new()),
            };
            Some((score, TextContact { matched, ..listed }))
        })
        .collect();
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| names::compare(&a.1.person.name, &b.1.person.name))
    });
    found.truncate(if typed.is_some() { FOUND } else { LISTED });
    let mut blocked: Vec<TextContact> = numbers
        .all_blocked()
        .map(|number| contact(number, Vec::new()))
        .collect();
    blocked.sort_by(|a, b| names::compare(&a.person.name, &b.person.name));
    Ok(TextContacts {
        contacts: found.into_iter().map(|(_, contact)| contact).collect(),
        blocked,
    })
}

/// The number `raw`'s conversation, opened with no text if it's new, for someone at the DSP to
/// text first.
pub fn open(db: &Store, state: &State, dsp: &str, raw: &str) -> Result<Conversation> {
    let number = texting::normal(raw).ok_or_else(|| Error::new("invalid_input", 400))?;
    offering(db, dsp)?;
    let opened = db.open_communications_text(dsp, &Filed::Number(number))?;
    one(db, state, dsp, &opened.id)
}

/// The work conversation of `driver`, one of today's drivers, opened with no text if it's new,
/// for someone at the DSP to text them first at the work phone they hold.
pub fn open_driver(db: &Store, state: &State, dsp: &str, driver: &str) -> Result<Conversation> {
    offering(db, dsp)?;
    let live =
        LiveDay::read(db, dsp)?.ok_or_else(|| Error::new("communications_routes_over", 409))?;
    ensure(
        live.drives(db, dsp, driver)?,
        "communications_driver_not_found",
        404,
    )?;
    let opened = db.open_communications_text(
        dsp,
        &Filed::Driver {
            source: live.source,
            id: driver.to_owned(),
        },
    )?;
    one(db, state, dsp, &opened.id)
}

/// Saves `name` for the number of the conversation `id`, as the member `by`.
pub fn save_contact(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    name: &str,
    by: &str,
) -> Result<Conversation> {
    let number = numbered(db, dsp, id)?;
    db.save_communications_contact(dsp, &number, name, by)?;
    live::changed(state, dsp);
    one(db, state, dsp, id)
}

/// Blocks the number of the conversation `id`, as the member `by`, or unblocks it with none.
pub fn block(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    by: Option<&str>,
) -> Result<Conversation> {
    let number = numbered(db, dsp, id)?;
    db.block_communications_number(dsp, &number, by)?;
    live::changed(state, dsp);
    one(db, state, dsp, id)
}
