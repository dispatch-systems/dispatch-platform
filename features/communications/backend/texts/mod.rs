//! Texting: the texts the DSP's dispatch phones relay, through whatever offers it `texting`
//! (Phones), each filed in a conversation (`filing`), kept in the DSP's own database
//! (`storage`), read by the page beside the Slack conversations, and answered by text (`reply`).
//! A work phone's texts are a driver's, across days, which Today's Routes will show; any other
//! number's are Messaging's: the number's conversation, with whoever Driver Match has the number
//! for, else the name the DSP saved for it (`contacts`), else the number itself.
pub(crate) mod contacts;
pub(crate) mod filing;
pub(crate) mod reply;
pub(crate) mod storage;

use super::{
    conversations::{PAGE, People},
    search,
    storage::Direction,
};
use crate::api::types::{
    Conversation, ConversationMessages, Conversations, Message, MessageFile, Sender,
};
use crate::backend::files::FileStatus;
use dispatch_core::{Error, Result, State, db::Store};
use dispatch_phones_contract as texting;
use std::collections::{BTreeSet, HashMap};
use storage::{ConversationKind, Text, TextConversation, TextStore};

/// Whether the DSP texts: something offers it `texting`.
pub fn offered(db: &Store, dsp: &str) -> Result<bool> {
    Ok(texting::for_dsp(db, dsp)?.is_some())
}

/// What the DSP keeps of the numbers that text it, and what its phones are called, to read its
/// texts by.
pub struct Numbers {
    contacts: HashMap<String, String>,
    blocked: BTreeSet<String>,
    phones: HashMap<String, String>,
}
impl Numbers {
    pub fn read(db: &Store, dsp: &str) -> Result<Self> {
        let phones = match texting::for_dsp(db, dsp)? {
            Some(texting) => texting
                .phones(db, dsp)?
                .into_iter()
                .map(|phone| (phone.id, phone.name))
                .collect(),
            None => HashMap::new(),
        };
        Ok(Self {
            contacts: db.communications_contacts(dsp)?.into_iter().collect(),
            blocked: db.communications_blocked(dsp)?,
            phones,
        })
    }
    /// The name the DSP saved for `number`, if it did.
    pub fn contact(&self, number: &str) -> Option<&str> {
        self.contacts.get(number).map(String::as_str)
    }
    pub fn blocked(&self, number: &str) -> bool {
        self.blocked.contains(number)
    }
}

/// `raw` written as `texting` keeps numbers, so one number is always written one way: `+1` and ten
/// digits in North America, `+` and its digits with a country code, a short code's digits alone;
/// none for what isn't a number.
pub fn normal(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.len() > 32
        || !raw
            .chars()
            .all(|c| c.is_ascii_digit() || " +-().".contains(c))
    {
        return None;
    }
    let plus = raw.starts_with('+');
    let bare = raw.chars().all(|c| c.is_ascii_digit());
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 if !plus => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        8..=15 if plus => Some(format!("+{digits}")),
        3..=7 if bare => Some(digits),
        _ => None,
    }
}

/// `number` as it reads: `(425) 555-0199` for a North American one, else as kept.
pub fn shown(number: &str) -> String {
    match number.strip_prefix("+1") {
        Some(ten) if ten.len() == 10 && ten.bytes().all(|b| b.is_ascii_digit()) => {
            format!("({}) {}-{}", &ten[..3], &ten[3..6], &ten[6..])
        }
        _ => number.to_owned(),
    }
}

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

/// `kept` as the page reads them, each with its latest text.
pub fn conversations(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    kept: Vec<TextConversation>,
) -> Result<Vec<Conversation>> {
    let ids: Vec<String> = kept.iter().map(|each| each.id.clone()).collect();
    let mut latest: HashMap<String, Text> = db
        .latest_communications_texts(dsp, &ids)?
        .into_iter()
        .map(|text| (text.conversation.clone(), text))
        .collect();
    Ok(kept
        .into_iter()
        .map(|conversation| {
            let last = latest.remove(&conversation.id);
            let number = conversation
                .number
                .clone()
                .or_else(|| last.as_ref().map(|text| text.number.clone()));
            let with = match &conversation.driver {
                Some((source, id)) => people.driver(source, id, id),
                None => {
                    let number = number.as_deref().unwrap_or_default();
                    people.texting(number, numbers.contact(number))
                }
            };
            let saved = conversation.kind == ConversationKind::Number;
            Conversation {
                id: conversation.id,
                kind: conversation.kind,
                team_id: None,
                contact: saved
                    && number
                        .as_deref()
                        .is_some_and(|n| numbers.contact(n).is_some()),
                blocked: saved && number.as_deref().is_some_and(|n| numbers.blocked(n)),
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
/// own, under its name, with each picture and file, and how one Dispatch sent went.
pub fn messages(people: &People, numbers: &Numbers, kept: Vec<Text>) -> Vec<Message> {
    kept.into_iter()
        .map(|text| Message {
            id: text.id.clone(),
            ts: place(&text),
            sender: (text.direction == Direction::Out).then(|| match &text.sent_by {
                Some(user) => people.sent_by(user, text.sender_name.as_deref().unwrap_or_default()),
                None => Sender {
                    user_id: None,
                    name: numbers
                        .phones
                        .get(&text.phone)
                        .cloned()
                        .unwrap_or_else(|| "Phone".to_owned()),
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
                    } else {
                        FileStatus::Waiting
                    },
                })
                .collect(),
            direction: text.direction,
            text: text.body,
            at: text.at,
            edited_at: None,
            in_thread: false,
            group: None,
            status: text.status,
        })
        .collect()
}

/// The numbers' conversations waiting, while the DSP texts, the longest waiting first: a
/// blocked number's aren't.
pub fn waiting(db: &Store, dsp: &str) -> Result<Vec<TextConversation>> {
    if !offered(db, dsp)? {
        return Ok(Vec::new());
    }
    db.waiting_communications_texts(dsp)
}

/// The numbers' conversations answered, while the DSP texts, before `before`, a page of them,
/// and whether there are more.
pub fn answered(
    db: &Store,
    people: &People,
    numbers: &Numbers,
    dsp: &str,
    before: Option<(i64, &str)>,
) -> Result<(Vec<Conversation>, bool)> {
    if !offered(db, dsp)? {
        return Ok((Vec::new(), false));
    }
    let mut kept = db.answered_communications_texts(dsp, before, PAGE + 1)?;
    let more = kept.len() > PAGE;
    kept.truncate(PAGE);
    Ok((conversations(db, people, numbers, dsp, kept)?, more))
}

/// The answered numbers' conversations before `before`, the time last written in and the ID.
pub fn older(db: &Store, state: &State, dsp: &str, before: (i64, &str)) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let (conversations, more) = answered(db, &people, &numbers, dsp, Some(before))?;
    Ok(Conversations {
        conversations,
        groups: Vec::new(),
        more,
    })
}

/// A conversation found by what was typed: how like it, and the letters of its name that matched.
type Found = (f64, Vec<(u32, u32)>, TextConversation);
/// The numbers' conversations whose name is like `typed`, or whose number holds the digits typed,
/// the closest first, then the latest, fifty at most; a blocked number's aren't.
pub fn search(db: &Store, state: &State, dsp: &str, typed: &str) -> Result<Conversations> {
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let mut found: Vec<Found> = Vec::new();
    if offered(db, dsp)? {
        for conversation in db.communications_numbers(dsp)? {
            let Some(number) = conversation.number.clone() else {
                continue;
            };
            if conversation.last_at.is_none() || numbers.blocked(&number) {
                continue;
            }
            let named = people.texting(&number, numbers.contact(&number));
            if let Some((score, marks)) = like(typed, &named.name, &number) {
                found.push((score, marks, conversation));
            }
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

/// How like `typed` someone named `name` texting from `number` is, and the letters of the name
/// that matched: digits typed find them by their number, anything else by their name.
fn like(typed: &str, name: &str, number: &str) -> Option<(f64, Vec<(u32, u32)>)> {
    let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
    let dialed = digits.len() >= 3
        && typed
            .chars()
            .all(|c| c.is_ascii_digit() || " +-().".contains(c));
    if dialed {
        return number.contains(&digits).then(|| (1.0, Vec::new()));
    }
    search::matched(typed, name)
}

/// The conversation `id` and its latest texts, or those before `before`.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    before: Option<(i64, &str)>,
) -> Result<ConversationMessages> {
    let kept = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    let mut read = db.communications_texts(dsp, id, before, PAGE + 1)?;
    let more = read.len() > PAGE;
    if more {
        read.remove(0);
    }
    Ok(ConversationMessages {
        conversation: conversations(db, &people, &numbers, dsp, vec![kept])?.remove(0),
        messages: messages(&people, &numbers, read),
        more,
    })
}

/// The conversation `id` as the page reads it.
pub fn one(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Conversation> {
    let kept = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    Ok(conversations(db, &people, &numbers, dsp, vec![kept])?.remove(0))
}

/// The text `id` as the page reads it.
pub fn message(db: &Store, state: &State, dsp: &str, id: &str) -> Result<Message> {
    let kept = db
        .communications_text(dsp, id)?
        .ok_or_else(|| Error::new("communications_message_not_found", 404))?;
    let people = People::read(db, state, dsp)?;
    let numbers = Numbers::read(db, dsp)?;
    Ok(messages(&people, &numbers, vec![kept]).remove(0))
}
