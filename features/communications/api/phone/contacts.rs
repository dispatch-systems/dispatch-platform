//! What changed of a phone's contacts, for a phone of a line that keeps its contacts on its phones:
//! the names it saved as its outbox handed them, and each number's name as its own contacts
//! changed. The line's page hears of each name that moved.
use super::access::{Phone, checked_in};
use crate::backend::{
    live,
    phone::{
        numbers::normal,
        storage::{ContactsReport, PhoneContactStore},
    },
    storage::DOMAIN,
    texts::{contacts::NAME_MOST, storage::ContactStore},
};
use dispatch_core::{
    Error, Result, State,
    db::now,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::Grant,
    },
};
use serde_json::Value;
use std::sync::Arc;

/// The most entries a phone reports of its contacts at once, of every kind.
const MOST_CONTACTS_REPORTED: usize = 200;

/// Keeps what the phone says changed of its contacts, refused as `phones_contacts_not_kept` for a
/// phone of Dispatch Phone's own line, whose names Dispatch keeps itself.
pub(super) async fn contacts(state: Arc<State>, input: Input, access: Phone) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let report = reported(&input.body)?;
    let caller = checked_in(&state, &input, access).await?;
    let line = caller
        .device
        .line
        .clone()
        .ok_or_else(|| Error::new("phones_contacts_not_kept", 409))?;
    let asked = input.clone();
    let written = line.clone();
    let changed = state
        .run_scoped(caller.dsp.clone(), DOMAIN, move |db| {
            let caller = access.authorize(db, &asked)?;
            let changed =
                db.phones_contacts_reported(&caller.dsp, &caller.device.id, &report, now())?;
            db.communications_numbers_changed(&caller.dsp, Some(&written), &changed)?;
            Ok(!changed.is_empty())
        })
        .await?;
    if changed {
        live::changed_on(&state, &caller.dsp, Some(&line));
    }
    Ok(Reply::ok())
}

/// What the phone reports, each entry of a number Dispatch takes and a name within bounds, others
/// ignored; refused as `invalid_input` for anything not shaped as the protocol has it, or with
/// none, or more than `MOST_CONTACTS_REPORTED`.
fn reported(body: &Value) -> Result<ContactsReport> {
    v::fields(body, &["saved", "changed", "removed"])?;
    let list = |key: &str| match &body[key] {
        Value::Null => Ok(&[][..]),
        Value::Array(entries) => Ok(entries.as_slice()),
        _ => Err(Error::new("invalid_input", 400)),
    };
    let (saved, changed, removed) = (list("saved")?, list("changed")?, list("removed")?);
    let count = saved.len() + changed.len() + removed.len();
    ensure(
        (1..=MOST_CONTACTS_REPORTED).contains(&count),
        "invalid_input",
        400,
    )?;
    let mut report = ContactsReport::default();
    for entry in saved {
        v::fields(entry, &["number", "at"])?;
        let at = v::integer(entry, "at", 0, i64::MAX)?;
        if let Some(number) = number(&entry["number"])? {
            report.saved.push((number, at));
        }
    }
    for entry in changed {
        v::fields(entry, &["number", "name"])?;
        let name = entry["name"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?
            .trim();
        if let Some(number) = number(&entry["number"])?
            && (1..=NAME_MOST).contains(&name.chars().count())
        {
            report.changed.push((number, name.to_owned()));
        }
    }
    for entry in removed {
        if let Some(number) = number(entry)? {
            report.removed.push(number);
        }
    }
    Ok(report)
}

/// A number as Dispatch writes it, none for one it doesn't take; refused for anything but text.
fn number(value: &Value) -> Result<Option<String>> {
    let raw = value
        .as_str()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    Ok(normal(raw))
}
