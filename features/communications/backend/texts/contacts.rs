//! Whom the DSP can text, and what someone at the DSP does with a number, each on one line:
//! Dispatch Phone's own (none), or another feature's, which keeps its own names and blocks.
//! Messaging's Contacts lists everyone Driver Match has a phone number for who hasn't left, by each
//! of them, and every number the line saved a name for, each number once, by name, with its
//! conversation on the line if it has one; then the numbers it blocked. A name typed, misspelt or
//! not, finds them as it finds conversations; digits typed, by their number. A number's
//! conversation, where a text to it written now is filed, or one of today's drivers', is opened to
//! text it first, and a number saved under a name, shared at the DSP, or blocked, or unblocked, on
//! its line; or on Dispatch Phone's own, added to a person Driver Match knows, or taken off them.
use super::{
    filing,
    live_day::LiveDay,
    model::Filed,
    numbers::{Numbers, shown},
    reading::{like, numbered, one},
    storage::{ContactStore, TextStore},
};
use crate::{
    api::types::{Conversation, TextContact, TextContacts, TextPeople},
    backend::{
        live,
        logged::together,
        people::People,
        phone::{numbers::normal, storage::Line},
    },
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::Store,
    ensure,
    foundation::names,
    manifest::people::{add_phone, remove_phone},
};
use std::collections::{HashMap, HashSet};

/// How many are listed with nothing typed, at most.
const LISTED: usize = 2000;
/// How many are found with something typed, at most.
const FOUND: usize = 20;
/// The most a contact's name may be, in characters.
pub const NAME_MOST: usize = 80;

/// Whom the DSP can text from the line `line`, all of them or those like `typed`, and the numbers
/// it blocked.
pub fn contacts(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    typed: Option<&str>,
) -> Result<TextContacts> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    let conversations: HashMap<String, (String, Option<i64>, Option<i64>)> = db
        .communications_numbers(dsp, line)?
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
        .flat_map(|person| person.phones.iter().filter_map(|phone| normal(phone)));
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

/// The conversation on the line `line` a text to the number `raw` written now belongs in, as
/// filing would place it, opened with no text if it's new, for someone at the DSP to text first: a
/// driver's latest work phone today is their conversation on Dispatch Phone's own; another line's
/// is always the number's.
pub fn open(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<Line>,
    raw: &str,
) -> Result<Conversation> {
    let line = line.map(|line| line.id);
    let number = normal(raw).ok_or_else(|| Error::new("invalid_input", 400))?;
    let filed = match line {
        None => filing::belongs(db, dsp, &number)?,
        Some(_) => Filed::Number(number),
    };
    let opened = db.open_communications_text(dsp, line, &filed)?;
    one(db, state, dsp, line, &opened.id)
}

/// The work conversation of `driver`, one of today's drivers, opened with no text if it's new,
/// for someone at the DSP to text them first at the work phone they have.
pub fn open_driver(db: &Store, state: &State, dsp: &str, driver: &str) -> Result<Conversation> {
    let live =
        LiveDay::read(db, dsp)?.ok_or_else(|| Error::new("communications_routes_over", 409))?;
    ensure(
        live.drives(db, dsp, driver)?,
        "communications_driver_not_found",
        404,
    )?;
    let opened = db.open_communications_text(
        dsp,
        None,
        &Filed::Driver {
            source: live.source,
            id: driver.to_owned(),
        },
    )?;
    one(db, state, dsp, None, &opened.id)
}

/// Saves `name` for the number of the conversation `id` of the line `line`, as the member `by`,
/// noted in the DSP's audit log as that line's, with the name it had before, if it had one.
pub fn save_contact(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
    name: &str,
    by: &str,
) -> Result<Conversation> {
    let number = numbered(db, dsp, line, id)?;
    let saved = together(db, dsp, || {
        let Some(before) = db.save_communications_contact(dsp, line, &number, name, by)? else {
            return Ok(false);
        };
        db.audit_with(
            Some(by),
            Some(dsp),
            &action(line, "contact_saved"),
            "",
            Some(&shown(&number)),
            &[("name", before, Some(name.to_owned()))],
        )?;
        Ok(true)
    })?;
    if saved {
        live::changed_on(state, dsp, line);
    }
    one(db, state, dsp, line, id)
}

/// Everyone Driver Match knows who hasn't left, by name, whom a number can be added to.
pub fn addable(db: &Store, state: &State, dsp: &str) -> Result<TextPeople> {
    let people = People::read(db, state, dsp)?;
    Ok(TextPeople {
        people: people
            .everyone()
            .iter()
            .filter(|person| !person.role.left())
            .map(People::known)
            .collect(),
    })
}

/// Adds the number of the conversation `id` to the person `code`'s, as the member `c` decided,
/// or with `added` false takes it off them, refused once it's someone else's.
pub fn add_to_person(
    db: &Store,
    state: &State,
    c: &Context,
    id: &str,
    code: &str,
    added: bool,
) -> Result<Conversation> {
    let dsp = c.dsp.id.as_str();
    let number = numbered(db, dsp, None, id)?;
    if added {
        add_phone(db, c, code, &number)?;
    } else {
        remove_phone(db, c, code, &number)?;
    }
    db.communications_number_changed(dsp, &number)?;
    live::changed(state, dsp);
    one(db, state, dsp, None, id)
}

/// Blocks the number of the conversation `id` of the line `line`, as the member `actor`, or
/// unblocks it, with `blocked` false, on that line alone, noted in the DSP's audit log as that
/// line's by whom it's with.
pub fn block(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
    actor: &str,
    blocked: bool,
) -> Result<Conversation> {
    let number = numbered(db, dsp, line, id)?;
    let by = blocked.then_some(actor);
    let changed = together(db, dsp, || {
        let changed = db.block_communications_number(dsp, line, &number, by)?;
        if changed {
            let what = if blocked {
                "number_blocked"
            } else {
                "number_unblocked"
            };
            // By the number, as its name's saves are, so the log gathers each of its own.
            let named = logged(&one(db, state, dsp, line, id)?.with.name, &number);
            db.audit_with(
                Some(actor),
                Some(dsp),
                &action(line, what),
                &named,
                Some(&shown(&number)),
                &[],
            )?;
        }
        Ok(changed)
    })?;
    if changed {
        live::changed_on(state, dsp, line);
    }
    one(db, state, dsp, line, id)
}

/// What the activity log calls `what` done on the line `line`: `communications.<what>` on Dispatch
/// Phone's own, `<line>.<what>` on another feature's, which words it, so a name saved on one line
/// is never logged as another's.
fn action(line: Option<&str>, what: &str) -> String {
    match line {
        None => format!("communications.{what}"),
        Some(line) => format!("{line}.{what}"),
    }
}

/// How a number reads in the audit log: by the name it's shown by, with the number, or the number
/// alone.
fn logged(name: &str, number: &str) -> String {
    let shown = shown(number);
    if name == shown || name == number {
        shown
    } else {
        format!("{name}, {shown}")
    }
}

#[cfg(test)]
#[path = "../../tests/backend/texts/contacts.rs"]
mod tests;
