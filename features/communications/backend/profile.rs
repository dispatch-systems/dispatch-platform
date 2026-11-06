//! The profile beside a conversation: what's known of whom it's with, besides the name, role and
//! photo the conversation has. Their route today and the work phone they hold now (`today`); and
//! how to reach them: their phone number and email address as Driver Match has them
//! (`directory`), and their Slack workspace: a Slack conversation's own, or for one by text, a
//! driver's work conversation or one with someone's own number, the first of the DSP's workspaces
//! they're in. Each feature is asked only
//! while it's on for the DSP, and what none knows is left out.
use super::{
    conversations,
    people::People,
    storage::CommunicationsStore,
    texts::{live_day::LiveDay, reading, storage::TextStore},
    today,
};
use crate::api::types::{Profile, Today};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    manifest::people::{self, Person},
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

/// The profile of whom the conversation by text `id` is with: the driver whose work conversation
/// it is, or whoever Driver Match has its number for; their Slack workspace, the first of the
/// DSP's they're in.
pub fn text(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Profile> {
    let conversation = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let person = reading::whose(&people, &conversation);
    let today = match (&conversation.driver, LiveDay::read(db, dsp)?) {
        (Some((source, driver)), Some(live)) if live.source == *source => {
            today::of_drivers(db, dsp, &live, std::slice::from_ref(driver))?
        }
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
