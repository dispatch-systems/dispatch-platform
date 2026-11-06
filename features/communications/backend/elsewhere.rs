//! A person's conversations of each kind, which the page goes between under the head of any of
//! them: their work conversation, while they work today, with the work phone they have now; the one by text with their own number, their default, the first of Driver
//! Match's numbers for them; and their Slack one, in a workspace
//! where Communications works, the latest written in, else their first account there. Each with
//! whether it waits, and what starts one they don't have yet; the page shows them while one is
//! another than the conversation open.
use super::{
    model::{ConversationKind, ConversationState},
    storage::conversations::ConversationStore,
    texts::{
        live_day::{self, LiveDay},
        model::TextConversation,
        numbers::work_number,
        storage::TextStore,
    },
    today,
};
use crate::{api::types::Elsewhere, backend::phone::numbers::normal, integrations::messages::ID};
use dispatch_core::{Result, db::Store, manifest::people::Person};
use dispatch_routes_contract as routes;
use dispatch_slack::kit;

/// Each of `person`'s conversations of a kind, work, by text and Slack, that they have.
pub fn of(db: &Store, dsp: &str, person: &Person) -> Result<Vec<Elsewhere>> {
    Ok([
        work(db, dsp, person)?,
        number(db, dsp, person)?,
        slack(db, dsp, person)?,
    ]
    .into_iter()
    .flatten()
    .collect())
}

/// `person`'s work conversation, shown while they work today, with a route today or a work phone
/// now, as the head of each of their conversations shows it (`today`), and have one or a work phone:
/// with one, that of the ID that has it (`LiveDay::phone_of`), which a text there goes to, started
/// if they have none by it; else the latest of any of their IDs, which on a day they don't work is
/// kept, hidden, so its update reads again what's read with it.
fn work(db: &Store, dsp: &str, person: &Person) -> Result<Option<Elsewhere>> {
    let live = LiveDay::read(db, dsp)?;
    let (source, routed, phone) = match &live {
        Some(live) => {
            let ids = person.ids_of(live.reader.source());
            if ids.is_empty() {
                return Ok(None);
            }
            let driven = live.routes_of(db, dsp, ids)?;
            let phone = live.phone_of(db, dsp, ids, &driven)?;
            (live.reader.source(), !driven.is_empty(), phone)
        }
        None => {
            let Some(reader) = routes::for_dsp(db, dsp)? else {
                return Ok(None);
            };
            let ids = person.ids_of(reader.source());
            if ids.is_empty() {
                return Ok(None);
            }
            let routed = today::driven(db, dsp, reader, ids)?.is_some();
            (reader.source(), routed, None)
        }
    };
    let ids = person.ids_of(source);
    let kept = live_day::slated(
        db,
        dsp,
        db.communications_drivers(dsp, source.as_str(), ids)?,
    )?;
    if !routed && phone.is_none() {
        return Ok(latest(kept).map(|each| Elsewhere {
            hidden: true,
            ..found(ConversationKind::Work, Some(each))
        }));
    }
    let conversation = match &phone {
        Some(phone) => kept.into_iter().find(|each| {
            each.driver
                .as_ref()
                .is_some_and(|(_, id)| *id == phone.driver_id)
        }),
        None => latest(kept),
    };
    let driver_id = match (&phone, &conversation) {
        (Some(phone), _) => Some(phone.driver_id.clone()),
        (None, Some(each)) => each.driver.as_ref().map(|(_, id)| id.clone()),
        (None, None) => return Ok(None),
    };
    Ok(Some(Elsewhere {
        number: phone.map(|phone| work_number(&phone.phone)),
        driver_id,
        ..found(ConversationKind::Work, conversation)
    }))
}

/// `person`'s conversation by text with their default number, the first Driver Match has for
/// them, while it has one: a number someone added to theirs before their sources', so a driver's
/// new phone is texted rather than the one they last wrote from.
fn number(db: &Store, dsp: &str, person: &Person) -> Result<Option<Elsewhere>> {
    let Some(default) = person.phones.iter().find_map(|phone| normal(phone)) else {
        return Ok(None);
    };
    let conversation = latest(db.communications_numbers_of(dsp, std::slice::from_ref(&default))?);
    Ok(Some(Elsewhere {
        number: Some(default),
        ..found(ConversationKind::Number, conversation)
    }))
}

/// `person`'s Slack conversation, in a workspace where Communications works.
fn slack(db: &Store, dsp: &str, person: &Person) -> Result<Option<Elsewhere>> {
    let on = kit::workspaces_on(db, dsp, ID)?;
    let accounts: Vec<(&str, &str)> = person
        .ids_of(kit::SOURCE)
        .iter()
        .filter_map(|id| id.split_once(':'))
        .filter(|(team, _)| on.iter().any(|each| each == team))
        .collect();
    let Some(&(team, user)) = accounts.first() else {
        return Ok(None);
    };
    let mut theirs = Vec::new();
    for (team, user) in &accounts {
        theirs.extend(db.communications_conversation_with(dsp, team, user)?);
    }
    let conversation = theirs.into_iter().max_by_key(|each| each.last_at);
    let (team, user) = match &conversation {
        Some(each) => (each.team.clone(), each.user.clone()),
        None => (team.to_owned(), user.to_owned()),
    };
    Ok(Some(Elsewhere {
        kind: ConversationKind::Slack,
        waiting: conversation
            .as_ref()
            .is_some_and(|each| each.state == ConversationState::Waiting),
        conversation_id: conversation.map(|each| each.id),
        number: None,
        driver_id: None,
        team_id: Some(team),
        user_id: Some(user),
        hidden: false,
    }))
}

/// The latest written in of `kept`.
fn latest(kept: Vec<TextConversation>) -> Option<TextConversation> {
    kept.into_iter().max_by_key(|each| each.last_at)
}
/// A conversation by text of `kind`, as found, if it was.
fn found(kind: ConversationKind, conversation: Option<TextConversation>) -> Elsewhere {
    Elsewhere {
        kind,
        waiting: conversation
            .as_ref()
            .is_some_and(|each| each.state == ConversationState::Waiting),
        conversation_id: conversation.map(|each| each.id),
        number: None,
        driver_id: None,
        team_id: None,
        user_id: None,
        hidden: false,
    }
}

#[cfg(test)]
#[path = "../tests/backend/elsewhere.rs"]
mod tests;
