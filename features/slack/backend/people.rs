//! The members of the DSP's Slack workspaces, as Driver Match reads them: one for each person,
//! by their email address, whichever of the DSP's workspaces they're in. Slack lists who the
//! DSP can reach, not whom it employs.
use super::storage::{Member, SlackStore};
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{Named, People, PeopleData, SourceData},
};
use std::collections::HashMap;

/// Slack's members, as IDs people are known by: `<workspace>:<member>`, the member's in the
/// workspace connected first.
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

/// One person for each email address across the workspaces, and for each member without one.
fn members(store: &Store, dsp: &str) -> Result<Vec<Named>> {
    let mut people: Vec<Person> = Vec::new();
    let mut by_email: HashMap<String, usize> = HashMap::new();
    for (team, member) in store.slack_members(dsp)? {
        let email = member.email.as_deref().map(str::to_lowercase);
        match email.as_ref().and_then(|email| by_email.get(email)) {
            Some(&index) => people[index].add(member),
            None => {
                if let Some(email) = email {
                    by_email.insert(email, people.len());
                }
                people.push(Person::new(&team, member));
            }
        }
    }
    Ok(people.into_iter().flat_map(Person::named).collect())
}

/// One person, as their members in the DSP's workspaces describe them.
struct Person {
    named: Named,
    /// The other names they go by in Slack.
    also: Vec<String>,
}
impl Person {
    /// The person `member` of the workspace `team` is, by their ID there.
    fn new(team: &str, member: Member) -> Self {
        let mut named = Named::new(&format!("{team}:{}", member.user));
        named.name = member.name.clone();
        let mut person = Self {
            named,
            also: vec![],
        };
        person.add(member);
        person
    }
    /// What another of their members says of them: listed while any account is active.
    fn add(&mut self, member: Member) {
        self.named.listed |= member.active;
        keep(&mut self.named.emails, member.email);
        keep(&mut self.named.phones, member.phone);
        for name in [Some(member.name), member.display_name]
            .into_iter()
            .flatten()
        {
            if name != self.named.name && !self.also.contains(&name) {
                self.also.push(name);
            }
        }
    }
    /// How Driver Match reads them: once for each name they go by, the first with all else.
    fn named(self) -> impl Iterator<Item = Named> {
        let id = self.named.id.clone();
        std::iter::once(self.named).chain(self.also.into_iter().map(move |name| Named {
            name,
            ..Named::new(&id)
        }))
    }
}
/// Adds `value` to `kept` unless it's there already.
fn keep(kept: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value.filter(|value| !kept.contains(value)) {
        kept.push(value);
    }
}
