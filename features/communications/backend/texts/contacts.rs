//! Whom the DSP can text, as Messaging's Contacts lists them: everyone Driver Match has a phone
//! number for who hasn't left, by their first, and every number the DSP saved a name for, each
//! number once, by name, with its conversation if it has one; then the numbers blocked. A name
//! typed, misspelt or not, finds them as it finds conversations; digits typed, by their number.
use super::{Numbers, like, normal, offered, storage::TextStore};
use crate::{
    api::types::{TextContact, TextContacts},
    backend::conversations::People,
};
use dispatch_core::{Result, State, db::Store, foundation::names};
use std::collections::{HashMap, HashSet};

/// How many are listed with nothing typed, and found with something, at most.
const LISTED: usize = 2000;
const FOUND: usize = 20;

/// Whom the DSP can text, all of them or those like `typed`, and the numbers blocked.
pub fn contacts(
    db: &Store,
    state: &State,
    dsp: &str,
    typed: Option<&str>,
) -> Result<TextContacts> {
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
        .filter_map(|each| {
            Some((
                each.number?,
                (each.id, each.waiting_since, each.last_at),
            ))
        })
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
        .filter_map(|person| person.phones.iter().find_map(|phone| normal(phone)));
    for number in theirs.chain(numbers.contacts.keys().cloned()) {
        if !numbers.blocked(&number) && seen.insert(number.clone()) {
            listed.push(number);
        }
    }
    let mut found: Vec<(f64, TextContact)> = listed
        .iter()
        .filter_map(|number| {
            let listed = contact(number, Vec::new());
            let (score, matched) = match typed {
                Some(typed) => like(typed, &listed.person.name, number)?,
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
        .blocked
        .iter()
        .map(|number| contact(number, Vec::new()))
        .collect();
    blocked.sort_by(|a, b| names::compare(&a.person.name, &b.person.name));
    Ok(TextContacts {
        contacts: found.into_iter().map(|(_, contact)| contact).collect(),
        blocked,
    })
}
