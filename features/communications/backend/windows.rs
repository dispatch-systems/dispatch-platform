//! Complete bounded reads. Cursors carry positions, never permission or a persistent snapshot.
use super::{
    changes::ChangeStore,
    conversations, dispatcher,
    model::{PAGE, Reference, WINDOW},
    people::People,
    storage::messages::MessageStore,
    texts::{self, numbers::Numbers, storage::TextStore},
};
use crate::api::types::{ConversationWindow, MessageRange};
use dispatch_core::{Error, Result, State, db::Store, ensure, foundation::crypto};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(crate) mod context;

/// The API's three request forms. A position need not still name an extant row.
pub enum Request {
    Latest,
    Anchor { place: String, leading: usize },
    Continue(String),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Position {
    anchor: Option<String>,
    leading: usize,
    revision: i64,
}

/// Selected raw rows. The two candidates contain at most WINDOW+1 each; only the selected
/// rows are hydrated. Left is descending, right ascending, and the pivot belongs to right.
pub(crate) struct Rows<T> {
    pub messages: Vec<T>,
    pub has_before: bool,
    pub has_after: bool,
}
pub(crate) fn select<T>(mut left: Vec<T>, mut right: Vec<T>, leading: usize) -> Rows<T> {
    let left_count = left.len().min(leading);
    let right_count = right.len().min(WINDOW - left_count);
    let left_count = left.len().min(WINDOW - right_count);
    let has_before = left.len() > left_count;
    let has_after = right.len() > right_count;
    left.truncate(left_count);
    left.reverse();
    left.extend(right.drain(..right_count));
    Rows {
        messages: left,
        has_before,
        has_after,
    }
}

/// Reads revision, selection, latest observation, bodies and attachments in one DSP
/// transaction. Directory/route context retains its existing independent owner semantics.
pub fn read(
    db: &Store,
    state: &State,
    dsp: &str,
    reference: &Reference,
    request: Request,
) -> Result<ConversationWindow> {
    read_inner(db, state, dsp, reference, request, false)
}
pub(crate) fn read_origins(
    db: &Store,
    state: &State,
    dsp: &str,
    reference: &Reference,
    request: Request,
) -> Result<ConversationWindow> {
    read_inner(db, state, dsp, reference, request, true)
}
fn read_inner(
    db: &Store,
    state: &State,
    dsp: &str,
    reference: &Reference,
    request: Request,
    want_origins: bool,
) -> Result<ConversationWindow> {
    let connection = db.dsp(dsp)?;
    connection.transaction(|| {
        // First Communications read establishes the snapshot before any message selection.
        let revision = db.communications_revision(dsp)?;
        dispatcher::found(db, dsp, reference)?;
        let binding =
            serde_json::to_string(&json!(["communications-window-v1", dsp, reference.id()]))?;
        let (position, rebased) = match request {
            Request::Latest => (
                Position {
                    anchor: None,
                    leading: 0,
                    revision,
                },
                false,
            ),
            Request::Anchor { place, leading } => (
                Position {
                    anchor: Some(place),
                    leading,
                    revision,
                },
                false,
            ),
            Request::Continue(token) => {
                let value = crypto::decrypt(&db.key, &binding, &token)
                    .map_err(|_| Error::new("invalid_input", 400))?;
                let position: Position =
                    serde_json::from_value(value).map_err(|_| Error::new("invalid_input", 400))?;
                ensure(position.revision >= 0, "invalid_input", 400)?;
                // A position ahead of this database requires an explicit bootstrap.
                // The caller must discard the token and bootstrap latest or an explicit anchor.
                ensure(
                    position.revision <= revision,
                    "communications_window_reset",
                    409,
                )?;
                let rebased = position.revision != revision;
                (position, rebased)
            }
        };
        ensure(position.leading < WINDOW, "invalid_input", 400)?;
        if let Some(anchor) = &position.anchor {
            checked(reference, anchor)?;
        } else {
            ensure(position.leading == 0, "invalid_input", 400)?;
        }
        let anchor = position
            .anchor
            .as_deref()
            .map(|place| (place, position.leading));
        let people = People::read(db, state, dsp)?;
        let numbers = match reference {
            Reference::Text(_) => Some(Numbers::read(db, dsp)?),
            _ => None,
        };
        let zone = want_origins.then(|| texts::zone(db, dsp)).transpose()?;
        let read_context =
            |messages: &[crate::api::types::Message], numbers: Option<&Numbers>| match zone {
                Some(zone) => context::read_in_zone(db, dsp, reference, messages, numbers, zone),
                None => context::read(db, dsp, reference, messages, numbers),
            };
        let (page, latest, context, latest_context, latest_workdays, has_before, has_after) =
            match reference {
                Reference::Slack(id) => {
                    let rows = db.communications_message_window(dsp, id, anchor)?;
                    let (before, after) = (rows.has_before, rows.has_after);
                    let page =
                        conversations::assembled_with(db, &people, dsp, id, rows.messages, before)?;
                    let latest = conversations::messages(
                        db,
                        &people,
                        dsp,
                        db.communications_messages(dsp, id, None, PAGE)?,
                    )?;
                    let context = read_context(&page.messages, None)?;
                    let latest_context = read_context(&latest, None)?;
                    (
                        page,
                        latest,
                        context,
                        latest_context,
                        Vec::new(),
                        before,
                        after,
                    )
                }
                Reference::Text(id) => {
                    let parsed = anchor
                        .map(|(place, leading)| {
                            texts::reading::before(place).map(|(at, id)| (at, id, leading))
                        })
                        .transpose()?;
                    let anchor = parsed
                        .as_ref()
                        .map(|(at, id, leading)| (*at, id.as_str(), *leading));
                    let rows = db.communications_text_window(dsp, id, anchor)?;
                    let (before, after) = (rows.has_before, rows.has_after);
                    let page = texts::reading::assembled_with(
                        db,
                        &people,
                        numbers
                            .as_ref()
                            .ok_or_else(|| Error::new("communications_origins_invariant", 500))?,
                        dsp,
                        id,
                        rows.messages,
                        before,
                    )?;
                    let numbers = numbers
                        .as_ref()
                        .ok_or_else(|| Error::new("communications_origins_invariant", 500))?;
                    let latest_read = db.communications_texts(dsp, id, None, PAGE)?;
                    let kept = db
                        .communications_text_conversation(dsp, id)?
                        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
                    let latest_workdays = match kept.driver {
                        Some((source, driver)) => {
                            texts::workdays::of(db, dsp, &source, &driver, &latest_read)?
                        }
                        None => Vec::new(),
                    };
                    let latest = texts::reading::messages(&people, numbers, latest_read);
                    let context = read_context(&page.messages, Some(numbers))?;
                    let latest_context = read_context(&latest, Some(numbers))?;
                    (
                        page,
                        latest,
                        context,
                        latest_context,
                        latest_workdays,
                        before,
                        after,
                    )
                }
            };
        let origins = zone
            .map(|zone| {
                super::origins::read(
                    &connection,
                    reference,
                    zone,
                    &page.messages,
                    &latest,
                    &people,
                    numbers.as_ref(),
                )
            })
            .transpose()?;
        let first = page.messages.first().map(|message| message.ts.clone());
        let last = page.messages.last().map(|message| message.ts.clone());
        let token = |anchor: Option<String>, leading| -> Result<String> {
            crypto::encrypt(
                &db.key,
                &binding,
                &serde_json::to_value(Position {
                    anchor,
                    leading,
                    revision,
                })?,
            )
        };
        // Navigation overlaps by one row. Progress uses stable transport ordering, never
        // status_at, local pending sends, or the separate latest observation.
        let before = if has_before {
            Some(token(first.clone(), WINDOW - 1)?)
        } else {
            None
        };
        let after = if has_after {
            Some(token(last.clone(), 0)?)
        } else {
            None
        };
        let refresh = token(position.anchor, position.leading)?;
        Ok(ConversationWindow {
            conversation: page.conversation,
            messages: page.messages,
            latest,
            context,
            latest_context,
            latest_workdays,
            origins,
            range: MessageRange {
                revision: revision.to_string(),
                first,
                last,
                has_before,
                has_after,
                before,
                after,
                refresh,
                rebased,
            },
            today: page.today,
            working: page.working,
            elsewhere: page.elsewhere,
            workdays: page.workdays,
            limits: page.limits,
        })
    })
}

/// Refuses `value` unless a window may anchor at it, a place held more strictly than a page's
/// `before`: text places are canonical, and Slack retains all six microsecond digits. Neither
/// transport is converted through f64.
fn checked(reference: &Reference, value: &str) -> Result<()> {
    match reference {
        Reference::Slack(_) => {
            ensure(
                value.len() <= 14 + super::phone::model::ID_MOST,
                "invalid_input",
                400,
            )?;
            conversations::seen(value)?;
            ensure(
                value
                    .split_once('.')
                    .is_some_and(|(_, micros)| micros.len() == 6),
                "invalid_input",
                400,
            )
        }
        Reference::Text(_) => {
            let (at, id) = texts::reading::before(value)?;
            ensure(
                value
                    .split_once(':')
                    .is_some_and(|(digits, _)| digits.len() == 13)
                    && (0..=9_999_999_999_999).contains(&at)
                    && super::phone::model::identifier(&id),
                "invalid_input",
                400,
            )
        }
    }
}

#[cfg(test)]
#[path = "../tests/backend/windows.rs"]
mod tests;
