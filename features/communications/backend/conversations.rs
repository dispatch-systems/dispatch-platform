//! The DSP's conversations as its pages read them: who each is with, as Driver Match knows them,
//! else as their Slack profile last named them; each message with who at the DSP sent it and its
//! files; and the workspaces they're in. Those waiting come first, the longest waiting first,
//! then the latest answered, of every workspace or one, fifty at a time; a conversation's
//! messages the latest fifty at a time, in the order written.
use super::{
    files::{File, FileStore},
    live, search,
    storage::{self, CommunicationsStore},
};
use crate::{
    api::types::{
        Communications, CommunicationsUpdates, CommunicationsWaiting, CommunicationsWorkspace,
        Conversation, ConversationMessages, Conversations, Correspondent, Message, MessageFile,
        Sender, Typing,
    },
    integrations::messages::{self, ID},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    manifest::people::{self, Everyone, Photo},
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
    /// The workspace `team`'s member `user`, whose Slack profile last named them `name`.
    pub fn correspondent(&self, team: &str, user: &str, name: &str) -> Correspondent {
        let person = self
            .everyone
            .holding(kit::SOURCE, &kit::member_id(team, user));
        Correspondent {
            user_id: user.to_owned(),
            name: person.map_or_else(|| name.to_owned(), |person| person.name.clone()),
            code: person.map(|person| person.code.clone()),
            role: person.map(|person| person.role),
            photo: person.and_then(|person| person.photo.clone()),
        }
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
        let (code, photo) = self.member(user);
        Some(Sender {
            user_id: user.clone(),
            name: message.sender_name.clone().unwrap_or_default(),
            code,
            photo,
        })
    }
    /// Who is writing, each as the person they are, if any.
    fn writing(&self, mut typing: Typing) -> Typing {
        for writer in &mut typing.writing {
            (writer.code, writer.photo) = self.member(&writer.user_id);
        }
        typing
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
            team_id: conversation.team,
            state: conversation.state,
            waiting_since: conversation.waiting_since,
            last_at: conversation.last_at,
            matched: Vec::new(),
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
    let mut files: HashMap<String, Vec<MessageFile>> = HashMap::new();
    for file in db.communications_files(dsp, &ids)? {
        let File {
            id,
            message,
            name,
            mime,
            size,
            status,
            ..
        } = file;
        files
            .entry(message.unwrap_or_default())
            .or_default()
            .push(MessageFile {
                id,
                name,
                mime,
                size,
                status,
            });
    }
    Ok(kept
        .into_iter()
        .map(|message| Message {
            sender: people.sender(&message),
            files: files.remove(&message.id).unwrap_or_default(),
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
        })
        .collect())
}

/// `GET /api/dsp/communications`, for the member `me`: every conversation waiting, as the counts
/// go by them, and the latest answered of the workspace `team`, or every one.
pub fn overview(
    db: &Store,
    state: &State,
    dsp: &str,
    me: &str,
    team: Option<&str>,
) -> Result<Communications> {
    let people = People::read(db, state, dsp)?;
    let revision = db.communications_revision(dsp)?;
    let waiting = db.waiting_communications(dsp)?;
    let count = waiting.len();
    let mut answered = db.answered_communications(dsp, team, None, PAGE + 1)?;
    let more = answered.len() > PAGE;
    answered.truncate(PAGE);
    Ok(Communications {
        revision,
        waiting: count,
        conversations: conversations(db, &people, dsp, [waiting, answered].concat())?,
        more,
        workspaces: workspaces(db, dsp)?,
        typing: people.writing(live::typing(dsp, me).0),
    })
}
/// The answered conversations, of the workspace `team` or every one, before `before`, its latest
/// message's time and its ID.
pub fn older(
    db: &Store,
    state: &State,
    dsp: &str,
    team: Option<&str>,
    before: (i64, &str),
) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let mut answered = db.answered_communications(dsp, team, Some(before), PAGE + 1)?;
    let more = answered.len() > PAGE;
    answered.truncate(PAGE);
    Ok(Conversations {
        conversations: conversations(db, &people, dsp, answered)?,
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
        more: false,
    })
}
/// The conversation `id` and its latest messages, or those before the one Slack knows as
/// `before`.
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
    Ok(ConversationMessages {
        conversation: conversations(db, &people, dsp, vec![kept])?.remove(0),
        messages: messages(db, &people, dsp, read)?,
        more,
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
    let waiting = db.waiting_communications(dsp)?.len();
    let ids = changed.as_ref().filter(|ids| !ids.is_empty());
    let mut conversations = Vec::new();
    // Who anyone is, only when there's someone to name.
    if ids.is_some() || !typing.writing.is_empty() {
        let people = People::read(db, state, dsp)?;
        typing = people.writing(typing);
        if let Some(ids) = ids {
            let kept = db.communications_conversations(dsp, ids)?;
            conversations = self::conversations(db, &people, dsp, kept)?;
        }
    }
    Ok((
        CommunicationsUpdates {
            revision,
            reset: changed.is_none(),
            waiting,
            conversations,
            typing,
            workspaces: workspaces(db, dsp)?,
        },
        next,
    ))
}
/// How many conversations wait, and since when each has.
pub fn waiting(db: &Store, dsp: &str) -> Result<CommunicationsWaiting> {
    let since: Vec<i64> = db
        .waiting_communications(dsp)?
        .into_iter()
        .filter_map(|conversation| conversation.waiting_since)
        .collect();
    Ok(CommunicationsWaiting {
        revision: db.communications_revision(dsp)?,
        count: since.len(),
        since,
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
