//! Whom someone at the DSP can message: every active member of the DSP's workspaces where
//! Communications works, or of one of them, as Driver Match knows them, with their photo and
//! role, and their conversation if they have one: whether it waits and when it was last written
//! in. By name, or found by a name typed, misspelt or not, the closest first.
use super::{
    people::People,
    search,
    storage::{self, CommunicationsStore},
};
use crate::{
    api::types::{Contact, Contacts},
    integrations::messages::ID,
};
use dispatch_core::{Result, State, db::Store, foundation::names};
use dispatch_slack::kit;
use std::collections::HashMap;

/// How many are listed with nothing typed, and found with something, at most: listed, every
/// member of a DSP's workspaces, which rarely hold more.
const LISTED: usize = 2000;
const FOUND: usize = 20;

/// The DSP's contacts, or those of the workspace `team`; all of them, or those like `typed`.
pub fn contacts(
    db: &Store,
    state: &State,
    dsp: &str,
    team: Option<&str>,
    typed: Option<&str>,
) -> Result<Contacts> {
    let people = People::read(db, state, dsp)?;
    let mut on = kit::workspaces_on(db, dsp, ID)?;
    on.retain(|each| team.is_none_or(|team| each == team));
    let mut conversations: HashMap<(String, String), storage::Conversation> = HashMap::new();
    for team in &on {
        for conversation in db.communications_in(dsp, team)? {
            conversations.insert(
                (conversation.team.clone(), conversation.user.clone()),
                conversation,
            );
        }
    }
    let mut found: Vec<(f64, Contact)> = kit::members(db, dsp)?
        .into_iter()
        .filter(|(team, member)| member.active && on.contains(team))
        .filter_map(|(team, member)| {
            let person = people.correspondent(&team, &member.user, &member.name);
            let (score, matched) = match typed {
                Some(typed) => search::matched(typed, &person.name)?,
                None => (0.0, Vec::new()),
            };
            let conversation = conversations.get(&(team.clone(), member.user));
            Some((
                score,
                Contact {
                    conversation_id: conversation.map(|conversation| conversation.id.clone()),
                    waiting_since: conversation.and_then(|conversation| conversation.waiting_since),
                    last_at: conversation.and_then(|conversation| conversation.last_at),
                    team_id: team,
                    person,
                    matched,
                },
            ))
        })
        .collect();
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| names::compare(&a.1.person.name, &b.1.person.name))
    });
    found.truncate(if typed.is_some() { FOUND } else { LISTED });
    Ok(Contacts {
        contacts: found.into_iter().map(|(_, contact)| contact).collect(),
    })
}

#[cfg(test)]
#[path = "../tests/backend/contacts.rs"]
mod tests;
