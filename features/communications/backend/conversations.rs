//! The DSP's conversations as its pages read them: who each is with, as Driver Match knows them,
//! else as their Slack profile last named them; each message with who at the DSP sent it, its
//! files and the group it went to; and the workspaces they're in. Those waiting come first, the
//! longest waiting first, with the groups waiting, then the latest answered and the groups among
//! them, of every workspace or one, fifty at a time; a conversation's messages the latest fifty at
//! a time, in the order written. The conversations by text come with them, as `texts` reads them,
//! while the DSP texts.
use super::{
    files::{File, FileStore},
    groups::{
        self,
        storage::{Answered, GroupStore},
    },
    live, search,
    storage::{self, CommunicationsStore, Direction},
    texts::{
        self, elsewhere,
        numbers::{Numbers, shown},
        reading,
        storage::{ConversationKind, TextStore},
        todays_routes,
    },
};
use crate::{
    api::types::{
        Communications, CommunicationsUpdates, CommunicationsWaiting, CommunicationsWorkspace,
        Conversation, ConversationMessages, Conversations, Correspondent, Group, Message,
        MessageFile, MessageGroup, Sender, Typing, WaitingMessage,
    },
    integrations::messages::{self, ID},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    manifest::people::{self, Everyone, Person, Photo, Source},
};
use dispatch_slack::kit;
use std::{collections::HashMap, sync::Arc};

/// How many answered conversations, or messages, are read at a time.
pub const PAGE: usize = 50;

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
    /// A work phone nobody held, `number`, by its last four digits.
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
    fn sender(&self, message: &storage::Message) -> Option<Sender> {
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
    fn writing(&self, mut typing: Typing) -> Typing {
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

/// `kept` as its pages read them, each with its latest message.
pub fn conversations(
    db: &Store,
    people: &People,
    dsp: &str,
    kept: Vec<storage::Conversation>,
) -> Result<Vec<Conversation>> {
    let ids: Vec<String> = kept
        .iter()
        .map(|conversation| conversation.id.clone())
        .collect();
    let last = db.latest_communications_messages(dsp, &ids)?;
    let of: Vec<String> = last
        .iter()
        .map(|message| message.conversation.clone())
        .collect();
    let mut latest: HashMap<String, Message> = of
        .into_iter()
        .zip(messages(db, people, dsp, last)?)
        .collect();
    Ok(kept
        .into_iter()
        .map(|conversation| Conversation {
            with: people.correspondent(&conversation.team, &conversation.user, &conversation.name),
            last: latest.remove(&conversation.id),
            id: conversation.id,
            kind: ConversationKind::Slack,
            team_id: Some(conversation.team),
            number: None,
            driver_id: None,
            contact: false,
            blocked: false,
            state: conversation.state,
            waiting_since: conversation.waiting_since,
            last_at: conversation.last_at,
            matched: Vec::new(),
            group_id: conversation.group,
        })
        .collect())
}
/// `kept` as its pages read them, each with its files.
pub fn messages(
    db: &Store,
    people: &People,
    dsp: &str,
    kept: Vec<storage::Message>,
) -> Result<Vec<Message>> {
    let ids: Vec<String> = kept.iter().map(|message| message.id.clone()).collect();
    let mut files = attached(db.communications_files(dsp, &ids)?, |file| {
        file.message.clone()
    });
    // The groups the DSP's went to.
    let sent_to: Vec<String> = kept
        .iter()
        .filter(|message| message.direction == Direction::Out)
        .filter_map(|message| message.group.clone())
        .collect();
    let groups: HashMap<String, String> = db
        .communications_groups(dsp, &sent_to)?
        .into_iter()
        .map(|group| (group.id, group.name))
        .collect();
    Ok(kept
        .into_iter()
        .map(|message| Message {
            sender: people.sender(&message),
            files: files.remove(&message.id).unwrap_or_default(),
            group: message
                .group
                .as_ref()
                .filter(|_| message.direction == Direction::Out)
                .and_then(|id| {
                    Some(MessageGroup {
                        name: groups.get(id)?.clone(),
                        id: id.clone(),
                    })
                }),
            in_thread: message
                .thread_ts
                .as_ref()
                .is_some_and(|thread| *thread != message.ts),
            id: message.id,
            ts: message.ts,
            direction: message.direction,
            text: message.text,
            at: message.at,
            edited_at: message.edited_at,
            status: None,
            number: None,
        })
        .collect())
}

/// `files` as a message shows them, by what `of` says they're attached to.
pub fn attached(
    files: Vec<File>,
    of: impl Fn(&File) -> Option<String>,
) -> HashMap<String, Vec<MessageFile>> {
    let mut attached: HashMap<String, Vec<MessageFile>> = HashMap::new();
    for file in files {
        let to = of(&file).unwrap_or_default();
        let File {
            id,
            name,
            mime,
            size,
            status,
            ..
        } = file;
        attached.entry(to).or_default().push(MessageFile {
            id,
            name,
            mime,
            size,
            status,
        });
    }
    attached
}

/// The answered conversations not listed under a group, and the groups nothing under waits in,
/// as the page reads them: of the workspace `team` or every one, before `before`, a page of them,
/// and whether there are more.
fn answered(
    db: &Store,
    people: &People,
    dsp: &str,
    team: Option<&str>,
    before: Option<(i64, &str)>,
) -> Result<(Vec<Conversation>, Vec<Group>, bool)> {
    let mut rows = db.answered_communications(dsp, team, before, PAGE + 1)?;
    let more = rows.len() > PAGE;
    rows.truncate(PAGE);
    let (mut conversations, mut found) = (Vec::new(), Vec::new());
    for row in rows {
        match row {
            Answered::Conversation(id) => conversations.push(id),
            Answered::Group(id) => found.push(id),
        }
    }
    let mut kept = db.communications_conversations(dsp, &conversations)?;
    kept.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| b.id.cmp(&a.id)));
    let mut listed = db.communications_groups(dsp, &found)?;
    listed.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| b.id.cmp(&a.id)));
    Ok((
        self::conversations(db, people, dsp, kept)?,
        groups::listed(db, people, dsp, listed)?,
        more,
    ))
}

/// `GET /api/dsp/communications`, for the member `me`: every conversation waiting, as the counts
/// go by them, and every group waiting; then the latest answered conversations and groups of the
/// workspace `team`, or every one, and the latest answered numbers' conversations.
pub fn overview(
    db: &Store,
    state: &State,
    dsp: &str,
    me: &str,
    team: Option<&str>,
) -> Result<Communications> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let revision = db.communications_revision(dsp)?;
    let waiting = db.waiting_communications(dsp)?;
    let texted = reading::waiting(db, dsp)?;
    let count = waiting.len() + texted.len();
    let waiting_groups = db.waiting_communications_groups(dsp)?;
    let (answered, answered_groups, more) = answered(db, &people, dsp, team, None)?;
    let (answered_texts, more_texts) = reading::answered(db, &people, &numbers, dsp, None)?;
    Ok(Communications {
        revision,
        waiting: count,
        conversations: [
            conversations(db, &people, dsp, waiting)?,
            reading::conversations(db, &people, &numbers, dsp, texted)?,
            answered,
            answered_texts,
        ]
        .concat(),
        groups: [
            groups::listed(db, &people, dsp, waiting_groups)?,
            answered_groups,
        ]
        .concat(),
        more,
        more_texts,
        workspaces: workspaces(db, dsp)?,
        texting: texts::offered(db, dsp)?,
        routes: todays_routes::offered(db, dsp)?,
        phones: texts::phones(db, dsp)?,
        typing: people.writing(live::typing(dsp, me).0),
    })
}
/// The answered conversations and groups, of the workspace `team` or every one, before `before`,
/// the time last written in and the ID.
pub fn older(
    db: &Store,
    state: &State,
    dsp: &str,
    team: Option<&str>,
    before: (i64, &str),
) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let (conversations, groups, more) = answered(db, &people, dsp, team, Some(before))?;
    Ok(Conversations {
        conversations,
        groups,
        more,
    })
}
/// A conversation found by a name typed: how like it its name is, and its letters that matched.
type Found = (f64, Vec<(u32, u32)>, storage::Conversation);
/// The conversations, of the workspace `team` or every one, whose names are like `typed`, the
/// closest first, fifty at most.
pub fn search(
    db: &Store,
    state: &State,
    dsp: &str,
    team: Option<&str>,
    typed: &str,
) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let mut found: Vec<Found> = db
        .communications_with_messages(dsp)?
        .into_iter()
        .filter(|conversation| team.is_none_or(|team| conversation.team == team))
        .filter_map(|conversation| {
            let named =
                people.correspondent(&conversation.team, &conversation.user, &conversation.name);
            let (score, matched) = search::matched(typed, &named.name)?;
            Some((score, matched, conversation))
        })
        .collect();
    // The closest first; of those as close, the latest.
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.2.last_at.cmp(&a.2.last_at)));
    found.truncate(PAGE);
    let (marks, kept): (Vec<_>, Vec<_>) = found
        .into_iter()
        .map(|(_, matched, kept)| (matched, kept))
        .unzip();
    let mut conversations = conversations(db, &people, dsp, kept)?;
    for (conversation, matched) in conversations.iter_mut().zip(marks) {
        conversation.matched = matched;
    }
    Ok(Conversations {
        conversations,
        groups: Vec::new(),
        more: false,
    })
}
/// The conversation `id` and its latest messages, or those before the one Slack knows as
/// `before`, with whom it's with's conversations of each kind.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    before: Option<&str>,
) -> Result<ConversationMessages> {
    let kept = db
        .communications_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let mut read = db.communications_messages(dsp, id, before, PAGE + 1)?;
    let more = read.len() > PAGE;
    if more {
        read.remove(0);
    }
    let elsewhere = match people.member_of(&kept.team, &kept.user) {
        Some(person) => elsewhere::of(db, dsp, person)?,
        None => Vec::new(),
    };
    Ok(ConversationMessages {
        conversation: conversations(db, &people, dsp, vec![kept])?.remove(0),
        messages: messages(db, &people, dsp, read)?,
        more,
        today: None,
        elsewhere,
        workdays: Vec::new(),
    })
}
/// The conversation `id` as its pages read it.
pub fn one(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Conversation> {
    let kept = db
        .communications_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    Ok(conversations(db, &people, dsp, vec![kept])?.remove(0))
}
/// What changed since the change `after`, for the member `me`, with the workspaces as they are
/// now, and when who is writing next changes on its own, if anyone is.
pub fn updates(
    db: &Store,
    state: &State,
    dsp: &str,
    after: i64,
    me: &str,
) -> Result<(CommunicationsUpdates, Option<i64>)> {
    let (changed, revision) = db.communications_changed_since(dsp, after)?;
    let (mut typing, next) = live::typing(dsp, me);
    let waiting = db.waiting_communications(dsp)?.len() + reading::waiting(db, dsp)?.len();
    let ids = changed.as_ref().filter(|ids| !ids.is_empty());
    let (mut conversations, mut changed_groups) = (Vec::new(), Vec::new());
    // Who anyone is, only when there's someone to name.
    if ids.is_some() || !typing.writing.is_empty() {
        let people = People::read(db, state, dsp)?;
        typing = people.writing(typing);
        if let Some(ids) = ids {
            // Each change names a conversation, a group, or a conversation by text.
            let kept = db.communications_conversations(dsp, ids)?;
            conversations = self::conversations(db, &people, dsp, kept)?;
            let kept = db.communications_groups(dsp, ids)?;
            changed_groups = groups::listed(db, &people, dsp, kept)?;
            let texted = db.communications_text_conversations(dsp, ids)?;
            let numbers = Numbers::read(db, dsp)?;
            conversations.extend(reading::conversations(db, &people, &numbers, dsp, texted)?);
        }
    }
    Ok((
        CommunicationsUpdates {
            revision,
            reset: changed.is_none(),
            waiting,
            conversations,
            groups: changed_groups,
            typing,
            workspaces: workspaces(db, dsp)?,
            texting: texts::offered(db, dsp)?,
            routes: todays_routes::offered(db, dsp)?,
            phones: texts::phones(db, dsp)?,
        },
        next,
    ))
}
/// How many conversations wait, since when each has, and the latest message from whom each is
/// with, the longest waiting first.
pub fn waiting(db: &Store, state: &State, dsp: &str) -> Result<CommunicationsWaiting> {
    let waiting = db.waiting_communications(dsp)?;
    let ids: Vec<String> = waiting.iter().map(|each| each.id.clone()).collect();
    let written = db.latest_communications_written(dsp, &ids)?;
    let of: Vec<String> = written
        .iter()
        .map(|each| each.conversation.clone())
        .collect();
    let people = People::read(db, state, dsp)?;
    let mut written: HashMap<String, Message> = of
        .into_iter()
        .zip(messages(db, &people, dsp, written)?)
        .collect();
    let mut latest: Vec<(Option<i64>, WaitingMessage)> = waiting
        .iter()
        .filter_map(|conversation| {
            Some((
                conversation.waiting_since,
                WaitingMessage {
                    message: written.remove(&conversation.id)?,
                    conversation_id: conversation.id.clone(),
                    group_id: conversation.group.clone(),
                    from: people
                        .correspondent(&conversation.team, &conversation.user, &conversation.name)
                        .name,
                },
            ))
        })
        .collect();
    let mut since: Vec<i64> = waiting
        .into_iter()
        .filter_map(|conversation| conversation.waiting_since)
        .collect();
    // The numbers' too, as the page lists them.
    let texted = reading::waiting(db, dsp)?;
    if !texted.is_empty() {
        let numbers = Numbers::read(db, dsp)?;
        let ids: Vec<String> = texted.iter().map(|each| each.id.clone()).collect();
        let mut written: HashMap<String, Message> = db
            .latest_communications_texts_in(dsp, &ids)?
            .into_iter()
            .map(|text| {
                let of = text.conversation.clone();
                (
                    of,
                    reading::messages(&people, &numbers, vec![text]).remove(0),
                )
            })
            .collect();
        for listed in reading::conversations(db, &people, &numbers, dsp, texted)? {
            since.extend(listed.waiting_since);
            if let Some(message) = written.remove(&listed.id) {
                latest.push((
                    listed.waiting_since,
                    WaitingMessage {
                        message,
                        conversation_id: listed.id,
                        group_id: None,
                        from: listed.with.name,
                    },
                ));
            }
        }
        since.sort();
        latest.sort_by_key(|(since, _)| *since);
    }
    Ok(CommunicationsWaiting {
        revision: db.communications_revision(dsp)?,
        count: since.len(),
        since,
        latest: latest.into_iter().map(|(_, message)| message).collect(),
    })
}
/// What the page calls a workspace: its name in Dispatch, else its name in Slack.
pub fn named(workspace: &kit::Workspace) -> String {
    workspace
        .label
        .clone()
        .unwrap_or_else(|| workspace.name.clone())
}
/// The workspace `team`'s logo, the copy Slack keeps of it: its kind, and its picture.
pub fn logo(db: &Store, dsp: &str, team: &str) -> Result<Option<Photo>> {
    kit::logo(db, dsp, team)
}
/// The DSP's workspaces, by name, each with whether Communications works there, the address of
/// the app's Messages tab in it, whether it needs updating for Communications, and its logo's
/// version.
pub fn workspaces(db: &Store, dsp: &str) -> Result<Vec<CommunicationsWorkspace>> {
    let on = kit::workspaces_on(db, dsp, ID)?;
    let mut logos = kit::logos(db, dsp)?;
    Ok(kit::workspaces(db, dsp)?
        .into_iter()
        .map(|workspace| {
            let on = on.contains(&workspace.team);
            CommunicationsWorkspace {
                on,
                needs_update: on && !messages::approved(&workspace),
                logo: logos.remove(&workspace.team),
                chat_link: format!(
                    "https://slack.com/app_redirect?app={}&team={}",
                    workspace.app, workspace.team
                ),
                name: named(&workspace),
                team_id: workspace.team,
            }
        })
        .collect())
}

#[cfg(test)]
#[path = "../tests/backend/conversations.rs"]
mod tests;
