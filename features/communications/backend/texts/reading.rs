//! Reading the DSP's conversations by text as the page reads them, beside the Slack ones: those
//! waiting, the answered a page at a time, those found by a name or a number's digits, and a
//! conversation with its texts, each under whoever wrote it, or for `windows` a bounded window
//! of them around a place. A work conversation waits only on the live day (`live_day`); a
//! driver's comes with their route today, each workday's route, and their conversations of
//! other kinds. Each read is of one line, Dispatch Phone's own (none) or another feature's: a
//! conversation of another line is never read, as though it weren't there.
use super::{
    live_day::{self, LiveDay, Stopped},
    model::{Text, TextConversation, TextStatus},
    numbers::Numbers,
    storage::TextStore,
    workdays,
};
use crate::api::types::{
    Conversation, ConversationMessages, Conversations, Message, MessageFile, Sender,
};
use crate::backend::{
    access::Access,
    bytes, compose, elsewhere,
    model::{ConversationKind, ConversationState, Direction, FileStatus, PAGE},
    people::People,
    phone::files,
    search, today,
    windows::Window,
};
use dispatch_core::manifest::people::Person;
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, now},
    server::http::input::Reply,
};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};

/// A text's place among its conversation's, as a message's `ts`: when it was written, thirteen
/// digits, and its ID.
fn place(text: &Text) -> String {
    format!("{:013}:{}", text.at, text.id)
}
/// Where a page of texts left off, `<at>:<id>`, as a message's `ts` says it.
pub fn before(value: &str) -> Result<(i64, String)> {
    value
        .split_once(':')
        .and_then(|(at, id)| {
            let valid = !at.is_empty() && at.bytes().all(|b| b.is_ascii_digit()) && !id.is_empty();
            Some((at.parse().ok().filter(|_| valid)?, id.to_owned()))
        })
        .ok_or_else(|| Error::new("invalid_input", 400))
}

/// `kept` as the page reads them, each with its latest text: a work conversation as of the live
/// day.
pub fn conversations(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    kept: Vec<TextConversation>,
) -> Result<Vec<Conversation>> {
    let kept = live_day::slated(db, dsp, kept)?;
    let ids: Vec<String> = kept.iter().map(|each| each.id.clone()).collect();
    let mut latest: HashMap<String, Text> = db
        .latest_communications_texts(dsp, &ids)?
        .into_iter()
        .filter_map(|text| Some((text.conversation.clone()?, text)))
        .collect();
    Ok(kept
        .into_iter()
        .map(|conversation| {
            let last = latest.remove(&conversation.id);
            let number = conversation
                .number
                .clone()
                .or_else(|| last.as_ref().map(|text| text.number.clone()));
            let number_or_none = number.as_deref().unwrap_or_default();
            let with = match (&conversation.driver, conversation.kind) {
                (Some((source, id)), _) => people.driver(source, id, id),
                (None, ConversationKind::Work) => {
                    people.work_phone(number_or_none, numbers.contact(number_or_none))
                }
                (None, _) => people.texting(number_or_none, numbers.contact(number_or_none)),
            };
            // A number's own conversation, or a work phone's nobody had, is saved as a contact as its
            // number is; a number's alone is blocked.
            let own = number
                .as_deref()
                .filter(|_| contactable(conversation.kind, conversation.driver.is_some()));
            let blocked = conversation.kind == ConversationKind::Number
                && own.is_some_and(|number| numbers.blocked(number));
            Conversation {
                id: conversation.id,
                kind: conversation.kind,
                driver_id: conversation.driver.map(|(_, id)| id),
                team_id: None,
                contact: own.is_some_and(|number| numbers.contact(number).is_some()),
                added: own.is_some_and(|number| people.everyone().added(number).is_some()),
                blocked,
                number,
                with,
                state: conversation.state,
                waiting_since: conversation.waiting_since,
                last_at: conversation.last_at,
                last: last.map(|text| messages(people, numbers, vec![text]).remove(0)),
                matched: Vec::new(),
                group_id: None,
            }
        })
        .collect())
}

/// `kept` as the page reads them: the DSP's under who sent each, by their account, or the phone's
/// own, under its name, with each picture and file, and how one Dispatch sent went, and why it
/// failed.
pub fn messages(people: &People, numbers: &Numbers, kept: Vec<Text>) -> Vec<Message> {
    kept.into_iter()
        .map(|text| (text.moved(), text))
        .map(|(moved, text)| Message {
            id: text.id.clone(),
            ts: place(&text),
            sender: (text.direction == Direction::Out).then(|| match &text.sent_by {
                Some(user) => people.sent_by(user, text.sender_name.as_deref().unwrap_or_default()),
                None => Sender {
                    user_id: None,
                    phone_id: Some(text.phone.clone()),
                    name: numbers.phone(&text.phone).unwrap_or("Phone").to_owned(),
                    code: None,
                    photo: None,
                },
            }),
            files: text
                .files
                .into_iter()
                .map(|file| MessageFile {
                    id: file.id,
                    name: file.name,
                    mime: file.mime,
                    size: file.size,
                    status: if file.kept {
                        FileStatus::Kept
                    } else if file.gone {
                        FileStatus::Gone
                    } else if file.on_phone {
                        FileStatus::OnPhone
                    } else {
                        FileStatus::Waiting
                    },
                    version: file.sha256.as_deref().map(bytes::version),
                    preview: file.preview,
                })
                .collect(),
            direction: text.direction,
            text: text.body,
            at: text.at,
            edited_at: None,
            in_thread: false,
            group: None,
            moved,
            gif: text.gif,
            error: text
                .error
                .filter(|_| text.status == Some(TextStatus::Failed)),
            status: text.status,
            number: Some(text.number),
        })
        .collect()
}

/// The conversations by text of the line `line` waiting, the longest waiting first: the numbers',
/// but a blocked number's, and on Dispatch Phone's own, the work conversations waiting on the live
/// day.
pub fn waiting(db: &Store, dsp: &str, line: Option<&str>) -> Result<Vec<TextConversation>> {
    let mut waiting = db.waiting_communications_texts(dsp, line)?;
    if line.is_none() {
        waiting.extend(live_day::waiting(db, dsp)?);
    }
    waiting.sort_by(|a, b| {
        a.waiting_since
            .cmp(&b.waiting_since)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(waiting)
}

/// The numbers' conversations of the line `line` answered before `before`, a page of them, and
/// whether there are more.
pub fn answered(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    line: Option<&str>,
    before: Option<(i64, &str)>,
) -> Result<(Vec<Conversation>, bool)> {
    let mut kept = db.answered_communications_texts(dsp, line, before, PAGE + 1)?;
    let more = kept.len() > PAGE;
    kept.truncate(PAGE);
    Ok((conversations(db, people, numbers, dsp, kept)?, more))
}

/// The work conversations not waiting on the live day before `before`, of any day, the latest
/// first, a page of them, and whether there are more: one waiting since a day before it, as
/// waiting counts only on the live day, among them, done. Those waiting on the live day are
/// listed as waiting, so it reads past as many as wait, for a page of the rest.
pub fn answered_work(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    before: Option<(i64, &str)>,
) -> Result<(Vec<Conversation>, bool)> {
    let waiting = live_day::waiting(db, dsp)?.len();
    let kept = db.latest_communications_work(dsp, before, PAGE + 1 + waiting)?;
    let mut answered: Vec<_> = live_day::slated(db, dsp, kept)?
        .into_iter()
        .filter(|each| each.state != ConversationState::Waiting)
        .collect();
    let more = answered.len() > PAGE;
    answered.truncate(PAGE);
    Ok((conversations(db, people, numbers, dsp, answered)?, more))
}

/// The answered conversations of `kind` of the line `line`, numbers' or work, before `before`, the
/// time last written in and the ID.
pub fn older(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    kind: ConversationKind,
    before: (i64, &str),
) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    let (conversations, more) = match kind {
        ConversationKind::Work => answered_work(db, &people, &numbers, dsp, Some(before))?,
        _ => answered(db, &people, &numbers, dsp, line, Some(before))?,
    };
    Ok(Conversations {
        conversations,
        groups: Vec::new(),
        more,
    })
}

/// A conversation found by what was typed: how like it, and the letters of its name that matched.
type Found = (f64, Vec<(u32, u32)>, TextConversation);
/// The conversations of `kind` of the line `line`, numbers' or work, whose name is like `typed`, or
/// whose number holds the digits typed, a work conversation's by any work phone its texts were
/// with, the closest first, then the latest, fifty at most; a blocked number's aren't.
pub fn search(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    kind: ConversationKind,
    typed: &str,
) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    let mut found: Vec<Found> = Vec::new();
    let (kept, mut texted_with) = match kind {
        ConversationKind::Work => (
            db.communications_work(dsp)?,
            db.communications_work_numbers(dsp)?,
        ),
        _ => (db.communications_numbers(dsp, line)?, HashMap::new()),
    };
    for conversation in kept {
        let number = conversation.number.clone().unwrap_or_default();
        if conversation.last_at.is_none() || numbers.blocked(&number) {
            continue;
        }
        let named = match (&conversation.driver, kind) {
            (Some((source, id)), _) => people.driver(source, id, id),
            (None, ConversationKind::Work) => people.work_phone(&number, numbers.contact(&number)),
            (None, _) => people.texting(&number, numbers.contact(&number)),
        };
        let with = texted_with
            .remove(&conversation.id)
            .unwrap_or_else(|| vec![number]);
        if let Some((score, marks)) = like(typed, &named.name, &with) {
            found.push((score, marks, conversation));
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.2.last_at.cmp(&a.2.last_at)));
    found.truncate(PAGE);
    let (marks, kept): (Vec<_>, Vec<_>) = found
        .into_iter()
        .map(|(_, matched, kept)| (matched, kept))
        .unzip();
    let mut conversations = conversations(db, &people, &numbers, dsp, kept)?;
    for (conversation, matched) in conversations.iter_mut().zip(marks) {
        conversation.matched = matched;
    }
    Ok(Conversations {
        conversations,
        groups: Vec::new(),
        more: false,
    })
}

/// How like `typed` someone named `name` texting from `numbers` is, and the letters of the name
/// that matched: digits typed find them by any of their numbers, anything else by their name.
pub fn like(typed: &str, name: &str, numbers: &[String]) -> Option<(f64, Vec<(u32, u32)>)> {
    let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
    let dialed = digits.len() >= 3
        && typed
            .chars()
            .all(|c| c.is_ascii_digit() || " +-().".contains(c));
    if dialed {
        let held = numbers.iter().any(|number| number.contains(&digits));
        return held.then(|| (1.0, Vec::new()));
    }
    search::matched(typed, name)
}

/// The conversation `id` of the line `line` and its latest texts, or those before `before`; a
/// driver's with their route today, or for the two hours after the stop the day's and the work
/// phone they had then, and each workday's; one with someone's own number with the route they drive
/// today; whether whom it's with is working; and their conversations of each kind.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
    before: Option<(i64, &str)>,
) -> Result<ConversationMessages> {
    let kept = found(db, dsp, line, id)?;
    let mut read = db.communications_texts(dsp, id, before, PAGE + 1)?;
    let more = read.len() > PAGE;
    if more {
        read.remove(0);
    }
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    assembled(db, &people, &numbers, dsp, kept, read, more)
}
/// The window of the conversation `id` of the line `line` around `anchor`, a text's place and how
/// many come before it, else its latest, with its latest texts apart from it and, a driver's, their
/// workdays.
pub(crate) fn window(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    line: Option<&str>,
    id: &str,
    anchor: Option<(&str, usize)>,
) -> Result<Window> {
    let kept = found(db, dsp, line, id)?;
    let parsed = anchor
        .map(|(place, leading)| before(place).map(|(at, id)| (at, id, leading)))
        .transpose()?;
    let anchor = parsed
        .as_ref()
        .map(|(at, id, leading)| (*at, id.as_str(), *leading));
    let rows = db.communications_text_window(dsp, id, anchor)?;
    let latest = db.communications_texts(dsp, id, None, PAGE)?;
    let latest_workdays = match &kept.driver {
        Some((source, driver)) => workdays::of(db, dsp, source, driver, &latest)?,
        None => Vec::new(),
    };
    let page = assembled(
        db,
        people,
        numbers,
        dsp,
        kept,
        rows.messages,
        rows.has_before,
    )?;
    Ok(Window {
        page,
        latest: messages(people, numbers, latest),
        latest_workdays,
        has_before: rows.has_before,
        has_after: rows.has_after,
    })
}

/// The conversation `kept` with `read`, the texts selected of it, a page's or a window's, and what
/// `opened` says of whom it's with: its workdays are those of the texts selected, not of its
/// latest. Another line's says nothing of their route or their conversations elsewhere, which are
/// Dispatch Phone's.
fn assembled(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    kept: TextConversation,
    read: Vec<Text>,
    more: bool,
) -> Result<ConversationMessages> {
    let person = whose(people, &kept).filter(|_| kept.line.is_none());
    let (today, working, workdays) = match &kept.driver {
        Some((source, driver)) => {
            let ids = std::slice::from_ref(driver);
            // Working only on a route as live route data holds today, not on the day stopped, by
            // this ID or another Driver Match knows them by; their route and phone, this ID's.
            let (today, working) = match LiveDay::read(db, dsp)? {
                Some(live) if live.source == *source => {
                    let (today, routed) = today::of_drivers(db, dsp, &live, ids)?;
                    let source = live.reader.source();
                    let working = match people.everyone().holding(source, driver) {
                        Some(person) if !routed => {
                            !live.routes_of(db, dsp, person.ids_of(source))?.is_empty()
                        }
                        _ => routed,
                    };
                    (today, working)
                }
                Some(_) => (None, false),
                None => match Stopped::read(db, dsp, now())? {
                    Some(stopped) if stopped.source == *source => {
                        (today::of_stopped(db, dsp, &stopped, ids)?, false)
                    }
                    _ => (None, false),
                },
            };
            (
                today,
                working,
                workdays::of(db, dsp, source, driver, &read)?,
            )
        }
        // Someone texting from their own number: the route they drive today, for its van.
        None => match person {
            Some(person) => {
                let (today, working) = today::of_working(db, dsp, person)?;
                (today, working, Vec::new())
            }
            None => (None, false, Vec::new()),
        },
    };
    let elsewhere = match person {
        Some(person) => elsewhere::of(db, dsp, person)?,
        None => Vec::new(),
    };
    Ok(ConversationMessages {
        conversation: conversations(db, people, numbers, dsp, vec![kept])?.remove(0),
        messages: messages(people, numbers, read),
        more,
        today,
        working,
        elsewhere,
        workdays,
        limits: compose::TEXT.into(),
    })
}
/// Whom the conversation `kept` is with, as Driver Match knows them: the driver, or whoever it
/// has the number for.
pub(crate) fn whose<'a>(people: &'a People, kept: &TextConversation) -> Option<&'a Person> {
    match (&kept.driver, kept.kind, &kept.number) {
        (Some((source, id)), _, _) => people.driver_of(source, id),
        (None, ConversationKind::Number, Some(number)) => people.holding_number(number),
        _ => None,
    }
}

/// The conversation `id` of the line `line` as the page reads it.
pub fn one(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
) -> Result<Conversation> {
    let kept = found(db, dsp, line, id)?;
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    Ok(conversations(db, &people, &numbers, dsp, vec![kept])?.remove(0))
}

/// The conversation the text `id`, written in the conversation `written_in` of the line `line`, is
/// filed in now, as the page reads it: `written_in` while it waits to be filed.
pub fn filed(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
    written_in: &str,
) -> Result<Conversation> {
    let kept = db
        .communications_text(dsp, id)?
        .ok_or_else(|| Error::new("communications_message_not_found", 404))?;
    one(
        db,
        state,
        dsp,
        line,
        kept.conversation.as_deref().unwrap_or(written_in),
    )
}

/// The text `id`, sent from a phone of the line `line`, as the page reads it.
pub fn message(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    id: &str,
) -> Result<Message> {
    let kept = db
        .communications_text(dsp, id)?
        .ok_or_else(|| Error::new("communications_message_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp, line)?;
    Ok(messages(&people, &numbers, vec![kept]).remove(0))
}

/// A text's picture or file, `id`, as the phone side keeps it, or its `preview`: a picture's own,
/// or the picture of the first frame of a video, of a text of `access`'s line, for the member `c`,
/// whose access is checked again once it's read: it may have ended while it was.
pub async fn file(
    state: &Arc<State>,
    c: &Context,
    access: Access,
    id: &str,
    version: Option<&str>,
    preview: bool,
) -> Result<Reply> {
    let line = access.line;
    let poster = match preview {
        true => files::preview(state, &c.dsp.id, line, id).await?,
        false => None,
    };
    let viewer = &c.auth.user.id;
    let file = match poster {
        Some(poster) => poster,
        None => files::shown(state, &c.dsp.id, line, id, viewer, version, preview)
            .await?
            .ok_or_else(|| Error::new("communications_file_not_found", 404))?,
    };
    let asked = c.clone();
    state.read(move |db| access.revalidate(db, &asked)).await?;
    Ok(file)
}

/// Asks the phone of a text's video `id` of `access`'s line, which it keeps, for its bytes, for the
/// member `c`, whose access is checked again in the write that asks, and waits a while for them:
/// whether they're kept.
pub async fn fetch(state: &Arc<State>, c: &Context, access: Access, id: &str) -> Result<Reply> {
    let asked = c.clone();
    let kept = files::fetch(state, &c.dsp.id, access.line, id, move |db| {
        access.revalidate(db, &asked).map(|_| ())
    })
    .await?;
    Reply::of(&json!({ "kept": kept }))
}
/// The DSP's conversation by text `id` of the line `line`: another line's isn't found.
pub(crate) fn found(
    db: &Store,
    dsp: &str,
    line: Option<&str>,
    id: &str,
) -> Result<TextConversation> {
    db.communications_text_conversation(dsp, id)?
        .filter(|conversation| conversation.line.as_deref() == line)
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))
}
/// The number of the DSP's conversation by text `id` of the line `line`, which someone at the DSP
/// blocks: refused for any but a number's.
pub fn numbered(db: &Store, dsp: &str, line: Option<&str>, id: &str) -> Result<String> {
    let conversation = found(db, dsp, line, id)?;
    match (conversation.kind, conversation.number) {
        (ConversationKind::Number, Some(number)) => Ok(number),
        _ => Err(Error::new("invalid_input", 400)),
    }
}
/// The number of the DSP's conversation by text `id` of the line `line`, which someone at the DSP
/// saves a contact for or adds to someone: refused for any but a number's or a work phone's nobody
/// had.
pub fn contact_number(db: &Store, dsp: &str, line: Option<&str>, id: &str) -> Result<String> {
    let conversation = found(db, dsp, line, id)?;
    match conversation.number {
        Some(number) if contactable(conversation.kind, conversation.driver.is_some()) => Ok(number),
        _ => Err(Error::new("invalid_input", 400)),
    }
}
/// Whether a conversation of `kind`, a driver's or not, is saved as a contact as its number is: a
/// number's, or a work phone's nobody had.
fn contactable(kind: ConversationKind, driver: bool) -> bool {
    match kind {
        ConversationKind::Number => true,
        ConversationKind::Work => !driver,
        ConversationKind::Slack => false,
    }
}

#[cfg(test)]
#[path = "../../tests/backend/texts/reading.rs"]
mod tests;
