//! What reaches the Communications page as it happens: each change to a conversation, and who at
//! the DSP is writing in which. A page waits on `updates`, a long poll as the dashboard's other
//! live pages use, which answers once a change it hasn't seen is kept, or who is writing changes,
//! or twenty seconds pass, so either reaches it within a second or two. Who is writing is kept
//! in memory alone, each for six seconds after their page last said so, or until they send.
use crate::api::types::{Typing, Writing};
use dispatch_core::{
    State,
    accounts::api::types::PublicUser,
    db::now,
    server::live::Topic,
};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, MutexGuard},
};

/// Communications' live channel: whoever waits on it wakes when a DSP's conversations change,
/// or who is writing in them does.
pub const TOPIC: Topic = Topic("communications");
/// How long someone counts as writing after their page last said so, in milliseconds.
const WRITING_MS: i64 = 6_000;

/// Wakes the pages waiting on the DSP's conversations.
pub fn changed(state: &State, dsp: &str) {
    state.topics.notify(TOPIC, dsp);
}

/// Someone writing in a conversation, until when.
struct Writer {
    conversation: String,
    user: PublicUser,
    until: i64,
}
/// Who is writing in each DSP's conversations, with the version of that, which changes as
/// anyone starts or stops.
#[derive(Default)]
struct Writers {
    version: u64,
    writing: Vec<Writer>,
}
fn writers() -> MutexGuard<'static, HashMap<String, Writers>> {
    static WRITERS: LazyLock<Mutex<HashMap<String, Writers>>> = LazyLock::new(Default::default);
    WRITERS.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Notes that `user` is writing in the DSP's conversation `conversation`, for the next few
/// seconds, waking the pages waiting when they've only now started.
pub fn writing(state: &State, dsp: &str, conversation: &str, user: &PublicUser) {
    let started = {
        let mut all = writers();
        let theirs = all.entry(dsp.to_owned()).or_default();
        let until = now() + WRITING_MS;
        match theirs
            .writing
            .iter_mut()
            .find(|writer| writer.conversation == conversation && writer.user.id == user.id)
        {
            Some(writer) => {
                writer.until = until;
                false
            }
            None => {
                theirs.writing.push(Writer {
                    conversation: conversation.to_owned(),
                    user: user.clone(),
                    until,
                });
                theirs.version += 1;
                true
            }
        }
    };
    if started {
        changed(state, dsp);
    }
}
/// Notes that `user` stopped writing in the DSP's conversation `conversation`, as on sending.
pub fn stopped(state: &State, dsp: &str, conversation: &str, user: &str) {
    let stopped = {
        let mut all = writers();
        let theirs = all.entry(dsp.to_owned()).or_default();
        let before = theirs.writing.len();
        theirs
            .writing
            .retain(|writer| !(writer.conversation == conversation && writer.user.id == user));
        let stopped = theirs.writing.len() < before;
        if stopped {
            theirs.version += 1;
        }
        stopped
    };
    if stopped {
        changed(state, dsp);
    }
}
/// Who is writing in the DSP's conversations now, but `except`, and when the next of them stops
/// counting as writing, if anyone is.
pub fn typing(dsp: &str, except: &str) -> (Typing, Option<i64>) {
    let mut all = writers();
    let theirs = all.entry(dsp.to_owned()).or_default();
    let at = now();
    let before = theirs.writing.len();
    theirs.writing.retain(|writer| writer.until > at);
    if theirs.writing.len() < before {
        theirs.version += 1;
    }
    let next = theirs.writing.iter().map(|writer| writer.until).min();
    let writing = theirs
        .writing
        .iter()
        .filter(|writer| writer.user.id != except)
        .map(|writer| Writing {
            conversation_id: writer.conversation.clone(),
            user_id: writer.user.id.clone(),
            first_name: writer.user.first_name.clone(),
            name: writer.user.name(),
        })
        .collect();
    (
        Typing {
            version: theirs.version,
            writing,
        },
        next,
    )
}
