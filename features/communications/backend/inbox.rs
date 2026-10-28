//! The list as the page reads it, of every section `sections` describes: as it opens
//! (`overview`), what changed in it since a page last read it (`updates`), and the conversations
//! waiting, for the sidebar and alerts (`waiting`). Each kind's conversations are read by its own
//! module: Slack's by `conversations`, the texts' by `texts::reading`.
use super::{
    changes::ChangeStore,
    conversations,
    groups::{self, storage::GroupStore},
    live,
    people::People,
    sections::{self, MESSAGING, Reading, SLACK, Section, TODAYS_ROUTES},
    storage::CommunicationsStore,
    texts::{self, numbers::Numbers, reading, storage::TextStore},
};
use crate::api::types::{
    Communications, CommunicationsUpdates, CommunicationsWaiting, Message, WaitingMessage,
};
use dispatch_core::{Result, State, db::Store};
use std::collections::HashMap;

/// `GET /api/dsp/communications`, for the member `me`: each section the DSP has, its
/// conversations waiting, as the counts go by them, and the groups waiting; then the latest
/// answered of each, Slack's of the workspace `team`, or every one.
pub fn overview(
    db: &Store,
    state: &State,
    dsp: &str,
    me: &str,
    team: Option<&str>,
) -> Result<Communications> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let reading = Reading {
        db,
        dsp,
        people: &people,
        numbers: &numbers,
        team,
    };
    let revision = db.communications_revision(dsp)?;
    let listed = sections::listed(&reading)?;
    // Where the page reads each section: whether it has it, and whether older are left to read.
    let has = |of: &Section| listed.iter().any(|(section, _)| section.id == of.id);
    let more = |of: &Section| {
        listed
            .iter()
            .any(|(section, listed)| section.id == of.id && listed.more)
    };
    let (more_slack, more_texts) = (more(&SLACK), more(&MESSAGING));
    let (texting, routes) = (has(&MESSAGING), has(&TODAYS_ROUTES));
    let mut waiting = Vec::new();
    let (mut answered, mut listed_groups) = (Vec::new(), Vec::new());
    for (_, listed) in listed {
        waiting.extend(listed.waiting);
        answered.extend(listed.answered);
        listed_groups.extend(listed.groups);
    }
    Ok(Communications {
        revision,
        waiting: waiting.len(),
        conversations: [waiting, answered].concat(),
        groups: listed_groups,
        more: more_slack,
        more_texts,
        workspaces: conversations::workspaces(db, dsp)?,
        texting,
        routes,
        phones: texts::phones(db, dsp)?,
        typing: people.writing(live::typing(dsp, me).0),
    })
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
    let waiting = sections::waiting(db, dsp)?;
    let ids = changed.as_ref().filter(|ids| !ids.is_empty());
    let (mut conversations, mut changed_groups) = (Vec::new(), Vec::new());
    // Who anyone is, only when there's someone to name.
    if ids.is_some() || !typing.writing.is_empty() {
        let people = People::read(db, state, dsp)?;
        typing = people.writing(typing);
        if let Some(ids) = ids {
            // Each change names a conversation, a group, or a conversation by text.
            let kept = db.communications_conversations(dsp, ids)?;
            conversations = conversations::conversations(db, &people, dsp, kept)?;
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
            workspaces: conversations::workspaces(db, dsp)?,
            texting: (MESSAGING.offered)(db, dsp)?,
            routes: (TODAYS_ROUTES.offered)(db, dsp)?,
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
        .zip(conversations::messages(db, &people, dsp, written)?)
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
            .filter_map(|text| {
                let of = text.conversation.clone()?;
                Some((
                    of,
                    reading::messages(&people, &numbers, vec![text]).remove(0),
                ))
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

#[cfg(test)]
#[path = "../tests/backend/inbox.rs"]
mod tests;
