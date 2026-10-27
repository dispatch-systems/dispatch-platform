//! The profile beside a conversation: what's known of whom it's with, besides the name, role and
//! photo the conversation has. Their route today and the work phone they hold now (`today`); and
//! how to reach them: their phone number and email address as Driver Match has them
//! (`directory`), and their Slack workspace: a Slack conversation's own, or for a driver's work
//! conversation, the first of the DSP's workspaces they're in. Each feature is asked only while
//! it's on for the DSP, and what none knows is left out. A number's conversation has none.
use super::{
    conversations,
    model::ConversationKind,
    storage::CommunicationsStore,
    texts::{live_day::LiveDay, storage::TextStore},
    today,
};
use crate::api::types::Profile;
use dispatch_core::{
    Error, Result, State,
    db::Store,
    manifest::people::{self, Person, Source},
};
use dispatch_slack::kit;

/// The profile of whom the conversation `id` is with: a Slack conversation, or by text
/// (`texted`) a driver's work conversation.
pub fn of(db: &Store, state: &State, dsp: &str, id: &str, texted: bool) -> Result<Profile> {
    let everyone = people::everyone(db, state, dsp)?;
    let workspaces = kit::workspaces(db, dsp)?;
    let (person, today, team) = if texted {
        let conversation = db
            .communications_text_conversation(dsp, id)?
            .filter(|conversation| conversation.kind == ConversationKind::Work)
            .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
        let (source, driver) = conversation
            .driver
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let person = Source::parse(&source).and_then(|source| everyone.holding(source, &driver));
        let today = match LiveDay::read(db, dsp)? {
            Some(live) if live.source == source => today::of_drivers(db, dsp, &live, &[driver])?,
            _ => person
                .map(|person| today::of(db, dsp, person))
                .transpose()?
                .flatten(),
        };
        // Their Slack workspace: the first of the DSP's they're in.
        let team = person.and_then(|person| {
            workspaces
                .iter()
                .find(|workspace| slack_member(person, &workspace.team))
                .map(|workspace| workspace.team.clone())
        });
        (person, today, team)
    } else {
        let conversation = db
            .communications_conversation(dsp, id)?
            .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
        let person = everyone.holding(
            kit::SOURCE,
            &kit::member_id(&conversation.team, &conversation.user),
        );
        let today = person
            .map(|person| today::of(db, dsp, person))
            .transpose()?
            .flatten();
        (person, today, Some(conversation.team))
    };
    let slack = team.and_then(|team| {
        workspaces
            .iter()
            .find(|workspace| workspace.team == team)
            .map(conversations::named)
    });
    Ok(Profile {
        today,
        phone: person.and_then(|person| person.phones.first().cloned()),
        email: person.and_then(|person| person.emails.first().cloned()),
        slack,
    })
}

/// Whether `person` is a member of the workspace `team`, as Driver Match knows them.
fn slack_member(person: &Person, team: &str) -> bool {
    person
        .ids_of(kit::SOURCE)
        .iter()
        .any(|id| id.split_once(':').is_some_and(|(of, _)| of == team))
}

#[cfg(test)]
#[path = "../tests/backend/profile.rs"]
mod tests;
