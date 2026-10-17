//! The members of the DSP's Slack workspaces, as Driver Match reads them: each member by
//! their own ID, and one email address in several workspaces as one person. Slack lists who
//! the DSP can reach, not whom it employs.
use super::storage::{Member, SlackStore};
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{Named, People, PeopleData, SourceData},
};
use std::collections::HashMap;

/// Slack's members, as IDs people are known by: `<workspace>:<member>`, which Slack never
/// changes.
pub const MEMBERS: SourceData = SourceData {
    id: "slack",
    label: "Slack",
    order: 30,
};

/// Everyone in the DSP's workspaces, listed while their account is active, with the email
/// address and phone number on their profile, by their name and by the one they go by in
/// Slack.
pub const PEOPLE: People = People::new(
    PeopleData {
        id: "slack",
        order: 70,
        label: "Slack",
        unit: "member",
        icon: "MessageSquare",
    },
    &MEMBERS,
    members,
)
.contacts_only();

/// Every member, once for each name they go by. A member of several workspaces is the same
/// person in each by their email address, so Driver Match keeps all their IDs with one person
/// whichever workspace it saw first.
fn members(store: &Store, dsp: &str) -> Result<Vec<Named>> {
    let members = store.slack_members(dsp)?;
    let mut by_email: HashMap<String, Vec<String>> = HashMap::new();
    for (team, member) in &members {
        if let Some(email) = &member.email {
            by_email
                .entry(email.to_lowercase())
                .or_default()
                .push(id(team, member));
        }
    }
    Ok(members
        .into_iter()
        .flat_map(|(team, member)| {
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
            named(id, member, same_as)
        })
        .collect())
}
/// The ID the member `member` of the workspace `team` is known by.
fn id(team: &str, member: &Member) -> String {
    format!("{team}:{}", member.user)
}
/// One member as Driver Match reads them: by their name with all else, then by the one they
/// go by in Slack.
fn named(id: String, member: Member, same_as: Vec<String>) -> impl Iterator<Item = Named> {
    let also = member.display_name.map(|name| Named {
        name,
        ..Named::new(&id)
    });
    let first = Named {
        name: member.name,
        listed: member.active,
        emails: member.email.into_iter().collect(),
        phones: member.phone.into_iter().collect(),
        same_as,
        ..Named::new(&id)
    };
    std::iter::once(first).chain(also)
}
