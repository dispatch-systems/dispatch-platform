//! What reaches the Communications page as it happens: each change to a conversation, and who at
//! the DSP is writing in which, each of one line: Dispatch Phone's own, Slack's among them, or
//! another feature's, whose page hears only its own. A page waits on `updates`, a long poll as the
//! dashboard's other live pages use, which answers once a change it hasn't seen is kept, or who is
//! writing changes, or twenty seconds pass, so either reaches it within a second or two. Who is
//! writing is kept in memory alone, each for six seconds after their page last said so, or until
//! they send.
use crate::api::types::{Typing, Writing};
use dispatch_core::{
    State,
    accounts::api::types::PublicUser,
    db::now,
    server::live::{ScopedListener, Topic},
};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, MutexGuard},
};
use tokio::sync::watch::{self, error::RecvError};

/// Communications' live channel: whoever waits on it wakes when a DSP's conversations of Dispatch
/// Phone's own line change, or who is writing in them does.
pub const TOPIC: Topic = Topic("communications");
/// Each other line's, by the line: whoever waits on one wakes only as that line's conversations
/// change, or who is writing in them does.
const LINES: Topic = Topic("communications.lines");
/// How long someone counts as writing after their page last said so, in milliseconds.
const WRITING_MS: i64 = 6_000;

/// Wakes the pages waiting on the DSP's conversations of Dispatch Phone's own line.
pub fn changed(state: &State, dsp: &str) {
    state.topics.notify(TOPIC, dsp);
}
/// Wakes the pages waiting on the DSP's conversations of the line `line`: Dispatch Phone's own for
/// none.
pub fn changed_on(state: &State, dsp: &str, line: Option<&str>) {
    match line {
        None => changed(state, dsp),
        Some(line) => state.topics.notify_scoped(LINES, dsp, line),
    }
}

/// What a page waiting on one line's conversations wakes by.
pub enum Listener {
    Own(watch::Receiver<u64>),
    Line(ScopedListener),
}
impl Listener {
    /// Waits for the line's next change: an error once its channel closed, as its DSP is deleted.
    pub async fn changed(&mut self) -> Result<(), RecvError> {
        match self {
            Self::Own(own) => own.changed().await,
            Self::Line(line) => line.changed().await,
        }
    }
}
/// Waits on the DSP's conversations of the line `line`, Dispatch Phone's own for none. Subscribe
/// before reading, so a change between the read and the wait still wakes it.
pub fn listen(state: &State, dsp: &str, line: Option<&str>) -> Listener {
    match line {
        None => Listener::Own(state.topics.subscribe(TOPIC, dsp)),
        Some(line) => Listener::Line(state.topics.subscribe_scoped(LINES, dsp, line)),
    }
}

/// Someone writing in a conversation, until when.
struct Writer {
    conversation: String,
    user: PublicUser,
    until: i64,
}
/// Who is writing in each DSP's conversations of one line, with the version of that, which changes
/// as anyone starts or stops.
#[derive(Default)]
struct Writers {
    version: u64,
    writing: Vec<Writer>,
}
/// Each DSP's writers, by the DSP and the line.
type Lines = HashMap<(String, Option<String>), Writers>;
fn writers() -> MutexGuard<'static, Lines> {
    static WRITERS: LazyLock<Mutex<Lines>> = LazyLock::new(Default::default);
    WRITERS.lock().unwrap_or_else(|poison| poison.into_inner())
}
fn of(dsp: &str, line: Option<&str>) -> (String, Option<String>) {
    (dsp.to_owned(), line.map(str::to_owned))
}

/// Notes that `user` is writing in the DSP's conversation `conversation` of the line `line`, for
/// the next few seconds, waking the pages waiting on that line when they've only now started.
pub fn writing(
    state: &State,
    dsp: &str,
    line: Option<&str>,
    conversation: &str,
    user: &PublicUser,
) {
    let started = {
        let mut all = writers();
        let theirs = all.entry(of(dsp, line)).or_default();
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
        changed_on(state, dsp, line);
    }
}
/// Notes that `user` stopped writing in the DSP's conversation `conversation` of the line `line`,
/// as on sending.
pub fn stopped(state: &State, dsp: &str, line: Option<&str>, conversation: &str, user: &str) {
    let stopped = {
        let mut all = writers();
        let theirs = all.entry(of(dsp, line)).or_default();
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
        changed_on(state, dsp, line);
    }
}
/// Who is writing in the DSP's conversations of the line `line` now, but `except`, and when the
/// next of them stops counting as writing, if anyone is. Who each is to Driver Match,
/// `conversations` adds.
pub fn typing(dsp: &str, line: Option<&str>, except: &str) -> (Typing, Option<i64>) {
    let mut all = writers();
    let theirs = all.entry(of(dsp, line)).or_default();
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
            code: None,
            photo: None,
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

#[cfg(test)]
#[path = "../tests/backend/live.rs"]
mod tests;
