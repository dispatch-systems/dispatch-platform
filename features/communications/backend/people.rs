//! Who the DSP's conversations are with and who at the DSP wrote in them, as every kind of
//! conversation shows them: the people Driver Match tells apart (`directory`), by their Slack
//! account, a driver's ID of a source, or a phone number, else by the name a conversation keeps
//! for them; and the DSP's members, by their email address.
use super::{storage, texts::numbers::shown};
use crate::api::types::{Correspondent, Sender, Typing};
use dispatch_core::{
    Result, State,
    db::Store,
    manifest::people::{self, Everyone, Person, Source},
};
use dispatch_slack::kit;
use std::{collections::HashMap, sync::Arc};

/// Who the DSP's conversations are with, and who at the DSP sent each message: everyone Driver
/// Match tells apart, and the DSP's members' email addresses, by their accounts.
pub struct People {
    everyone: Arc<Everyone>,
    emails: HashMap<String, String>,
}
impl People {
    pub fn read(db: &Store, state: &State, dsp: &str) -> Result<Self> {
        Ok(Self {
            everyone: people::everyone(db, state, dsp)?,
            emails: db
                .members(dsp)?
                .into_iter()
                .map(|member| (member.user_id, member.email))
                .collect(),
        })
    }
    /// Everyone Driver Match tells apart.
    pub fn everyone(&self) -> &Everyone {
        &self.everyone
    }
    /// The workspace `team`'s member `user`, as Driver Match knows them, if it does.
    pub fn member_of(&self, team: &str, user: &str) -> Option<&Person> {
        self.everyone
            .holding(kit::SOURCE, &kit::member_id(team, user))
    }
    /// The workspace `team`'s member `user`, whose Slack profile last named them `name`.
    pub fn correspondent(&self, team: &str, user: &str, name: &str) -> Correspondent {
        Correspondent {
            user_id: Some(user.to_owned()),
            ..with(self.member_of(team, user), name)
        }
    }
    /// The person Driver Match has the phone number `number` for, if any: of several, one who
    /// hasn't left first, then by name.
    pub fn holding_number(&self, number: &str) -> Option<&Person> {
        let having = self.everyone.by_phone(number);
        having
            .iter()
            .find(|person| !person.role.left())
            .or(having.first())
            .copied()
    }
    /// Whoever texts from `number`: the person Driver Match has it for, else by the name the DSP
    /// saved for it, `contact`, else by the number.
    pub fn texting(&self, number: &str, contact: Option<&str>) -> Correspondent {
        let name = contact.map_or_else(|| shown(number), str::to_owned);
        with(self.holding_number(number), &name)
    }
    /// The driver holding `id` of the source `source`, as Driver Match knows them, if it does.
    pub fn driver_of(&self, source: &str, id: &str) -> Option<&Person> {
        Source::parse(source).and_then(|source| self.everyone.holding(source, id))
    }
    /// The driver holding `id` of the source `source`, as Driver Match knows them, else by the
    /// name `name`.
    pub fn driver(&self, source: &str, id: &str, name: &str) -> Correspondent {
        with(self.driver_of(source, id), name)
    }
    /// The driver holding `id` of `source`, as a reader of routes names its drivers, as Driver
    /// Match knows them, else by that ID.
    pub fn routed(&self, source: Source, id: &str) -> Correspondent {
        with(self.everyone.holding(source, id), id)
    }
    /// A work phone nobody had, `number`, by its last four digits.
    pub fn work_phone(&self, number: &str) -> Correspondent {
        let digits: String = number.chars().filter(char::is_ascii_digit).collect();
        with(
            None,
            &format!("Work phone {}", &digits[digits.len().saturating_sub(4)..]),
        )
    }
    /// The person the DSP's member `user` is, by their email address, if any: their code and
    /// their photo's version.
    fn member(&self, user: &str) -> (Option<String>, Option<String>) {
        let person = self
            .emails
            .get(user)
            .and_then(|email| self.everyone.find(email).into_iter().next());
        (
            person.map(|person| person.code.clone()),
            person.and_then(|person| person.photo.clone()),
        )
    }
    /// Who at the DSP sent `message`, as the person their email address is, if any.
    pub fn sender(&self, message: &storage::Message) -> Option<Sender> {
        let user = message.sent_by.as_ref()?;
        Some(self.sent_by(user, message.sender_name.as_deref().unwrap_or_default()))
    }
    /// The DSP's member `user`, who sent a message under the name `name`, as the person their
    /// email address is, if any.
    pub fn sent_by(&self, user: &str, name: &str) -> Sender {
        let (code, photo) = self.member(user);
        Sender {
            user_id: Some(user.to_owned()),
            phone_id: None,
            name: name.to_owned(),
            code,
            photo,
        }
    }
    /// Who is writing, each as the person they are, if any.
    pub fn writing(&self, mut typing: Typing) -> Typing {
        for writer in &mut typing.writing {
            (writer.code, writer.photo) = self.member(&writer.user_id);
        }
        typing
    }
}

/// Whom a conversation is with: `person`, as Driver Match knows them, else by `name`.
fn with(person: Option<&Person>, name: &str) -> Correspondent {
    Correspondent {
        user_id: None,
        name: person.map_or_else(|| name.to_owned(), |person| person.name.clone()),
        code: person.map(|person| person.code.clone()),
        role: person.map(|person| person.role),
        title: person.and_then(|person| person.title),
        photo: person.and_then(|person| person.photo.clone()),
    }
}
