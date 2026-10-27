//! Messaging a group: one message to many members of the DSP's workspaces, each getting it in
//! their own direct message with the app under the name and photo of whoever sent it, and their
//! answers read together. A group is listed among the conversations; the conversations of its
//! people are listed under it while it's what the DSP last wrote to them in and all they wait on
//! answers a message to a group. An answer is what one of them wrote after a message to the
//! group, until the DSP next writes to them alone.
//!
//! - `storage`: groups, their people, their messages, and each message going to each of them.
//! - `choices`: whom a group message can go to, and the groups to choose from.
//! - `send`: making a group and messaging it, and sending each message to each of its people in
//!   the background, within Slack's limits, picking up where it was after a restart.
pub(crate) mod choices;
pub(crate) mod send;
pub(crate) mod storage;

use super::{
    conversations,
    files::FileStore,
    model::PAGE,
    people::People,
    storage::{self as kept, CommunicationsStore},
};
use crate::api::types::{
    Group, GroupEntry, GroupMessages, GroupPeople, GroupPerson, GroupProgress,
};
use dispatch_core::{Error, Result, State, db::Store, foundation::names, text_enum};
use std::collections::HashMap;
use storage::{DeliveryStatus, GroupMessage, GroupStore, Outline, Progress};

/// How many of a group's people show it, as two faces.
const FACES: usize = 2;

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// What a group to choose from is: everyone on a route today, a wave of them, a role,
    /// everyone the DSP can message, or a workspace's members.
    pub enum GroupChoiceKind {
        Route => "route",
        Wave => "wave",
        Role => "role",
        Everyone => "everyone",
        Workspace => "workspace",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Where a group stands with one of its people: they answered it; or its latest message is
    /// going to them, went, or couldn't.
    pub enum GroupPersonState {
        Replied => "replied",
        Sending => "sending",
        Sent => "sent",
        NotSent => "not_sent",
    }
}

/// `kept` as the list reads them: how many people each has and the first two, the workspaces
/// they're in, and what was last written in it.
pub fn listed(
    db: &Store,
    people: &People,
    dsp: &str,
    kept: Vec<storage::Group>,
) -> Result<Vec<Group>> {
    let ids: Vec<String> = kept.iter().map(|group| group.id.clone()).collect();
    let mut outlines: HashMap<String, Outline> = db
        .communications_group_outlines(dsp, &ids, FACES)?
        .into_iter()
        .map(|outline| (outline.group.clone(), outline))
        .collect();
    let sent = db.latest_communications_group_messages(dsp, &ids)?;
    let answers = db.latest_communications_answers(dsp, &ids)?;
    let mut last: HashMap<String, GroupEntry> = HashMap::new();
    for entry in entries(db, people, dsp, sent, answers)? {
        let group = entry.0;
        let later = last
            .get(&group)
            .is_none_or(|kept| (entry.1.at, &entry.1.id) > (kept.at, &kept.id));
        if later {
            last.insert(group, entry.1);
        }
    }
    Ok(kept
        .into_iter()
        .map(|group| {
            let outline = outlines.remove(&group.id).unwrap_or_default();
            Group {
                people: outline.people,
                faces: outline
                    .faces
                    .iter()
                    .map(|member| people.correspondent(&member.team, &member.user, &member.name))
                    .collect(),
                team_ids: outline.teams,
                state: group.state,
                waiting_since: group.waiting_since,
                last_at: group.last_at,
                last: last.remove(&group.id),
                name: group.name,
                id: group.id,
            }
        })
        .collect())
}

/// The DSP's messages to groups and the answers, as a group's page reads them, each with its
/// group, in the order written: the DSP's under who sent them, with their photos and how far each
/// has gone; the answers under who wrote them, with their photos and files and their
/// conversation.
fn entries(
    db: &Store,
    people: &People,
    dsp: &str,
    sent: Vec<GroupMessage>,
    answers: Vec<kept::Message>,
) -> Result<Vec<(String, GroupEntry)>> {
    let sent_ids: Vec<String> = sent.iter().map(|message| message.id.clone()).collect();
    let mut photos =
        conversations::attached(db.communications_group_files(dsp, &sent_ids)?, |file| {
            file.group_message.clone()
        });
    let progress: HashMap<String, Progress> = db
        .communications_group_progress(dsp, &sent_ids)?
        .into_iter()
        .collect();
    let answer_ids: Vec<String> = answers.iter().map(|message| message.id.clone()).collect();
    let mut files = conversations::attached(db.communications_files(dsp, &answer_ids)?, |file| {
        file.message.clone()
    });
    let of: Vec<String> = answers
        .iter()
        .map(|message| message.conversation.clone())
        .collect();
    let writers: HashMap<String, kept::Conversation> = db
        .communications_conversations(dsp, &of)?
        .into_iter()
        .map(|conversation| (conversation.id.clone(), conversation))
        .collect();
    let mut entries: Vec<(String, GroupEntry)> = sent
        .into_iter()
        .map(|message| {
            let gone = progress.get(&message.id).copied().unwrap_or_default();
            let entry = GroupEntry {
                at: message.at,
                text: message.text,
                edited_at: None,
                files: photos.remove(&message.id).unwrap_or_default(),
                sender: Some(people.sent_by(&message.sent_by, &message.sender_name)),
                progress: Some(GroupProgress {
                    people: gone.people,
                    sent: gone.sent,
                    failed: gone.failed,
                }),
                from: None,
                conversation_id: None,
                id: message.id,
            };
            (message.group, entry)
        })
        .collect();
    for message in answers {
        let Some(group) = message.group.clone() else {
            continue;
        };
        let writer = writers.get(&message.conversation);
        entries.push((
            group,
            GroupEntry {
                at: message.at,
                text: message.text,
                edited_at: message.edited_at,
                files: files.remove(&message.id).unwrap_or_default(),
                sender: None,
                progress: None,
                from: writer.map(|conversation| {
                    people.correspondent(&conversation.team, &conversation.user, &conversation.name)
                }),
                conversation_id: Some(message.conversation),
                id: message.id,
            },
        ));
    }
    entries.sort_by(|a, b| (a.1.at, &a.1.id).cmp(&(b.1.at, &b.1.id)));
    Ok(entries)
}

/// The group `id` as the list reads it.
pub fn one(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Group> {
    let kept = db
        .communications_group(dsp, id)?
        .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    Ok(listed(db, &people, dsp, vec![kept])?.remove(0))
}

/// The group `id` and its latest messages and answers, or those before `before`, the time
/// written and the ID.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    before: Option<(i64, &str)>,
) -> Result<GroupMessages> {
    let kept = db
        .communications_group(dsp, id)?
        .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let (sent, answers) = db.communications_group_entries(dsp, id, before, PAGE + 1)?;
    let mut entries: Vec<GroupEntry> = entries(db, &people, dsp, sent, answers)?
        .into_iter()
        .map(|(_, entry)| entry)
        .collect();
    let more = entries.len() > PAGE;
    if more {
        entries.remove(0);
    }
    Ok(GroupMessages {
        group: listed(db, &people, dsp, vec![kept])?.remove(0),
        entries,
        more,
    })
}

/// Who the group `id` went to, who first sent it and when, and where it stands with each: those
/// who answered first, the first to first, then the rest by name.
pub fn people(db: &Store, state: &State, dsp: &str, id: &str) -> Result<GroupPeople> {
    let messages = db.communications_group_messages(dsp, id)?;
    let (Some(first), Some(latest)) = (messages.first(), messages.last()) else {
        return Err(Error::new("communications_group_not_found", 404));
    };
    let people = People::read(db, state, dsp)?;
    let answered: HashMap<(String, String), i64> = db
        .communications_group_answered(dsp, id)?
        .into_iter()
        .map(|(team, user, at)| ((team, user), at))
        .collect();
    let gone: HashMap<(String, String), DeliveryStatus> = db
        .communications_deliveries(dsp, &latest.id)?
        .into_iter()
        .map(|(team, user, status)| ((team, user), status))
        .collect();
    let members = db.communications_group_people(dsp, id)?;
    let mut teams: Vec<&str> = members.iter().map(|member| member.team.as_str()).collect();
    teams.sort_unstable();
    teams.dedup();
    let mut conversations: HashMap<(String, String), String> = HashMap::new();
    for team in teams {
        for conversation in db.communications_in(dsp, team)? {
            conversations.insert((conversation.team, conversation.user), conversation.id);
        }
    }
    let mut listed: Vec<(Option<i64>, GroupPerson)> = members
        .into_iter()
        .map(|member| {
            let key = (member.team.clone(), member.user.clone());
            let replied = answered.get(&key).copied();
            let state = match (replied, gone.get(&key)) {
                (Some(_), _) => GroupPersonState::Replied,
                (None, Some(DeliveryStatus::Sent)) => GroupPersonState::Sent,
                (None, Some(DeliveryStatus::Failed)) => GroupPersonState::NotSent,
                (None, _) => GroupPersonState::Sending,
            };
            let person = GroupPerson {
                person: people.correspondent(&member.team, &member.user, &member.name),
                conversation_id: conversations.remove(&key),
                team_id: member.team,
                state,
            };
            (replied, person)
        })
        .collect();
    listed.sort_by(|a, b| match (a.0, b.0) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => names::compare(&a.1.person.name, &b.1.person.name),
    });
    Ok(GroupPeople {
        sent_by: first.sender_name.clone(),
        sent_at: first.at,
        people: listed.into_iter().map(|(_, person)| person).collect(),
    })
}

/// Marks done every conversation listed under the group `id` that waits, as the member `by`: the
/// group, as the list reads it.
pub fn done(db: &Store, state: &State, dsp: &str, id: &str, by: &str) -> Result<Group> {
    db.communications_group(dsp, id)?
        .ok_or_else(|| Error::new("communications_group_not_found", 404))?;
    if db.mark_communications_group_done(dsp, id, by)? > 0 {
        super::live::changed(state, dsp);
    }
    one(db, state, dsp, id)
}

#[cfg(test)]
#[path = "../../tests/backend/groups.rs"]
mod tests;
