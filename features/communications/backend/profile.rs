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
use crate::api::types::{Profile, Today};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    manifest::people::{self, Person, Source},
};
use dispatch_slack::kit;

/// The profile of the member the Slack conversation `id` is with.
pub fn member(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Profile> {
    let conversation = db
        .communications_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let everyone = people::everyone(db, state, dsp)?;
    let person = everyone.holding(
        kit::SOURCE,
        &kit::member_id(&conversation.team, &conversation.user),
    );
    let today = person
        .map(|person| today::of(db, dsp, person))
        .transpose()?
        .flatten();
    let workspaces = kit::workspaces(db, dsp)?;
    Ok(profile(
        person,
        today,
        &workspaces,
        Some(&conversation.team),
    ))
}

/// The profile of the driver whose work conversation by text `id` is: their Slack workspace, the
/// first of the DSP's they're in.
pub fn driver(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Profile> {
    let conversation = db
        .communications_text_conversation(dsp, id)?
        .filter(|conversation| conversation.kind == ConversationKind::Work)
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let (source, driver) = conversation
        .driver
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let everyone = people::everyone(db, state, dsp)?;
    let person = Source::parse(&source).and_then(|source| everyone.holding(source, &driver));
    let today = match LiveDay::read(db, dsp)? {
        Some(live) if live.source == source => today::of_drivers(db, dsp, &live, &[driver])?,
        _ => person
            .map(|person| today::of(db, dsp, person))
            .transpose()?
            .flatten(),
    };
    let workspaces = kit::workspaces(db, dsp)?;
    let team = person.and_then(|person| {
        workspaces
            .iter()
            .find(|workspace| slack_member(person, &workspace.team))
            .map(|workspace| workspace.team.clone())
    });
    Ok(profile(person, today, &workspaces, team.as_deref()))
}

/// The profile of `person`, driving `today`, in the workspace `team` of the DSP's `workspaces`.
fn profile(
    person: Option<&Person>,
    today: Option<Today>,
    workspaces: &[kit::Workspace],
    team: Option<&str>,
) -> Profile {
    let slack = team.and_then(|team| {
        workspaces
            .iter()
            .find(|workspace| workspace.team == team)
            .map(conversations::named)
    });
    Profile {
        today,
        phone: person.and_then(|person| person.phones.first().cloned()),
        email: person.and_then(|person| person.emails.first().cloned()),
        slack,
    }
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
