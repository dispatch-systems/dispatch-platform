//! The members of the DSP's Slack workspaces, as Driver Match reads them: each member by
//! their own ID, and one email address in several workspaces as one person. Slack lists who
//! the DSP can reach, not whom it employs.
use super::storage::{Member, SlackStore};
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{KindInfo, Named, People, Source, SourceInfo},
};
use std::collections::HashMap;

/// Slack's members, as IDs people are known by: `<workspace>:<member>`, which Slack never
/// changes.
pub const SOURCE: Source = Source::new(&SourceInfo {
    id: "slack",
    label: "Slack",
    order: 30,
});

/// Everyone in the DSP's workspaces, listed while their account is active, with the email
/// address and phone number on their profile, by their name.
pub const PEOPLE: People = People::new(
    KindInfo {
        id: "slack",
        order: 70,
        label: "Slack",
        icon: "MessageSquare",
    },
    SOURCE,
    members,
)
.contacts_only();

/// Every member of the DSP's workspaces, by their name. A member of several workspaces is the
/// same person in each by their email address, so Driver Match keeps all their IDs with one
/// person whichever workspace it saw first. A workspace whose access broke lists nobody until
/// it's reconnected, as who is there now is unknown.
fn members(store: &Store, dsp: &str) -> Result<Vec<Named>> {
    let broken: HashMap<String, bool> = store
        .slack_workspaces(dsp)?
        .into_iter()
        .map(|workspace| (workspace.team, workspace.broken))
        .collect();
    let members: Vec<(String, Member, bool)> = store
        .slack_members(dsp)?
        .into_iter()
        .filter_map(|(team, member)| {
            let broken = *broken.get(&team)?;
            Some((team, member, broken))
        })
        .collect();
    let mut by_email: HashMap<String, Vec<String>> = HashMap::new();
    for (team, member, _) in &members {
        if let Some(email) = &member.email {
            by_email
                .entry(email.to_lowercase())
                .or_default()
                .push(id(team, member));
        }
    }
    Ok(members
        .into_iter()
        .map(|(team, member, broken)| {
            let id = id(&team, &member);
            let same_as = member
                .email
                .as_ref()
                .and_then(|email| by_email.get(&email.to_lowercase()))
                .into_iter()
                .flatten()
                .filter(|other| **other != id)
                .cloned()
                .collect();
            // The name someone goes by in Slack, as "Cam" or "Jordan K.", could be another's
            // full name somewhere, so only the name on their profile is read.
            Named {
                name: member.name,
                listed: member.active && !broken,
                emails: member.email.into_iter().collect(),
                phones: member.phone.into_iter().collect(),
                same_as,
                ..Named::new(&id)
            }
        })
        .collect())
}
/// The ID the member `member` of the workspace `team` is known by.
pub(super) fn id(team: &str, member: &Member) -> String {
    super::kit::member_id(team, &member.user)
}
