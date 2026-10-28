//! A person's conversations of each kind, which the page goes between under the head of any of
//! them: their work conversation, as a driver who held the DSP's work phones, with the work phone
//! they hold now; the one by text with their own number, the latest written in of Driver Match's
//! numbers for them, else the first; and their Slack one, in a workspace
//! where Communications works, the latest written in, else their first account there. Each with
//! whether it waits, and what starts one they don't have yet; listed only with two or more.
use super::{
    model::{ConversationKind, ConversationState},
    storage::CommunicationsStore,
    texts::{
        live_day::{self, LiveDay},
        model::TextConversation,
        numbers::work_number,
        storage::TextStore,
    },
};
use crate::{api::types::Elsewhere, backend::phone::numbers::normal, integrations::messages::ID};
use dispatch_core::{Result, db::Store, manifest::people::Person};
use dispatch_routes_contract as routes;
use dispatch_slack::kit;

/// Each of `person`'s conversations of a kind, work, by text and Slack, while there are two or
/// more.
pub fn of(db: &Store, dsp: &str, person: &Person) -> Result<Vec<Elsewhere>> {
    let found: Vec<Elsewhere> = [
        work(db, dsp, person)?,
        number(db, dsp, person)?,
        slack(db, dsp, person)?,
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(if found.len() > 1 { found } else { Vec::new() })
}

/// `person`'s work conversation, while they have one or hold a work phone now: holding one, that
/// of the ID holding it, which a text there goes to, started if they have none by it; else the
/// latest of any of their IDs.
fn work(db: &Store, dsp: &str, person: &Person) -> Result<Option<Elsewhere>> {
    let live = LiveDay::read(db, dsp)?;
    let source = match &live {
        Some(live) => live.reader.source(),
        None => match routes::for_dsp(db, dsp)? {
            Some(reader) => reader.source(),
            None => return Ok(None),
        },
    };
    let ids = person.ids_of(source);
    if ids.is_empty() {
        return Ok(None);
    }
    let kept = live_day::slated(
        db,
        dsp,
        db.communications_drivers(dsp, source.as_str(), ids)?,
    )?;
    let holding = match &live {
        Some(live) => live.held_by(db, dsp, ids)?,
        None => None,
    };
    let conversation = match &holding {
        Some(held) => kept.into_iter().find(|each| {
            each.driver
                .as_ref()
                .is_some_and(|(_, id)| *id == held.driver_id)
        }),
        None => latest(kept),
    };
    let driver_id = match (&holding, &conversation) {
        (Some(held), _) => Some(held.driver_id.clone()),
        (None, Some(each)) => each.driver.as_ref().map(|(_, id)| id.clone()),
        (None, None) => return Ok(None),
    };
    Ok(Some(Elsewhere {
        number: holding.map(|held| work_number(&held.phone)),
        driver_id,
        ..found(ConversationKind::Work, conversation)
    }))
}

/// `person`'s conversation by text with their own number, while Driver Match has one for them.
fn number(db: &Store, dsp: &str, person: &Person) -> Result<Option<Elsewhere>> {
    let numbers: Vec<String> = person
        .phones
        .iter()
        .filter_map(|phone| normal(phone))
        .collect();
    let Some(first) = numbers.first().cloned() else {
        return Ok(None);
    };
    let conversation = latest(db.communications_numbers_of(dsp, &numbers)?);
    Ok(Some(Elsewhere {
        number: Some(
            conversation
                .as_ref()
                .and_then(|each| each.number.clone())
                .unwrap_or(first),
        ),
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
    }
}

#[cfg(test)]
#[path = "../tests/backend/elsewhere.rs"]
mod tests;
