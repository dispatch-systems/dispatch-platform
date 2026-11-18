//! One conversation of either transport, by its reference (`model::Reference`): reading it, the
//! profile beside it, sending in it, marking it Done and saying who's writing in it each go
//! through here, to Slack's (`conversations`, `reply`) or texting's (`texts`). What only one
//! transport has, as a number's contact or Retry, goes to it directly. A new transport adds its arm
//! to each. Each is of one line: Dispatch Phone's own (none), Slack's among it, or another
//! feature's, whose conversations are all by text; a conversation of another line isn't found.
use super::{
    conversations, live,
    model::Reference,
    profile, reply,
    storage::conversations::ConversationStore,
    texts::{self, storage::TextStore},
};
use crate::api::types::{Conversation, ConversationMessages, MessageGif, Profile, SentMessage};
use dispatch_core::{
    Result, State, accounts::Context, db::Store, ensure, server::http::route::Dsp,
};
use std::sync::Arc;

/// The conversation `reference` of the line `line` and its latest messages, or those before the
/// one at `before`, as each transport places its messages.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    reference: &Reference,
    before: Option<&str>,
) -> Result<ConversationMessages> {
    match reference {
        Reference::Slack(id) => {
            own(line)?;
            let before = before.map(conversations::before).transpose()?;
            conversations::opened(db, state, dsp, id, before)
        }
        Reference::Text(id) => {
            let before = before.map(texts::reading::before).transpose()?;
            let before = before.as_ref().map(|(at, id)| (*at, id.as_str()));
            texts::reading::opened(db, state, dsp, line, id, before)
        }
    }
}

/// What the profile beside the conversation `reference` shows of whom it's with.
pub fn profile(db: &Store, state: &State, dsp: &str, reference: &Reference) -> Result<Profile> {
    match reference {
        Reference::Slack(id) => profile::member(db, state, dsp, id),
        Reference::Text(id) => profile::text(db, state, dsp, id),
    }
}

/// Marks the conversation `reference` of the line `line` done without an answer, as the member
/// `by`, while it waits, covering only what they were shown: up to the message at `seen`, as each
/// transport places its messages, else everything. The conversation, as the page reads it.
pub fn done(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    reference: &Reference,
    by: &str,
    seen: Option<&str>,
) -> Result<Conversation> {
    found(db, dsp, line, reference)?;
    let marked = match reference {
        Reference::Slack(id) => {
            let seen = seen.map(conversations::seen).transpose()?;
            db.mark_communications_done(dsp, id, by, seen)?
        }
        Reference::Text(id) => {
            let seen = seen.map(texts::reading::before).transpose()?;
            let seen = seen.as_ref().map(|(at, id)| (*at, id.as_str()));
            db.mark_communications_text_done(dsp, id, by, seen)?
        }
    };
    if marked {
        live::changed_on(state, dsp, line);
    }
    one(db, state, dsp, line, reference)
}

/// The conversation `reference` of the line `line`, as the page reads it.
pub fn one(
    db: &Store,
    state: &State,
    dsp: &str,
    line: Option<&str>,
    reference: &Reference,
) -> Result<Conversation> {
    match reference {
        Reference::Slack(id) => {
            own(line)?;
            conversations::one(db, state, dsp, id)
        }
        Reference::Text(id) => texts::reading::one(db, state, dsp, line, id),
    }
}

/// Refuses the conversation `reference` unless it's the DSP's, of the line `line`.
pub fn found(db: &Store, dsp: &str, line: Option<&str>, reference: &Reference) -> Result<()> {
    let found = match reference {
        Reference::Slack(id) => {
            line.is_none() && db.communications_conversation(dsp, id)?.is_some()
        }
        Reference::Text(id) => db
            .communications_text_conversation(dsp, id)?
            .is_some_and(|conversation| conversation.line.as_deref() == line),
    };
    ensure(found, "communications_conversation_not_found", 404)
}
/// Refuses a Slack conversation on another line than Dispatch Phone's own, which alone has Slack's.
fn own(line: Option<&str>) -> Result<()> {
    ensure(line.is_none(), "communications_conversation_not_found", 404)
}

/// Sends what the member `c` wrote in the conversation `reference` of the line `line`, with the
/// photos they added for it, or a GIF in their place (`adding`): the message, as the page reads
/// it, and the conversation it's in now, which by text is another once the number it went to
/// changed hands as it went. Their access is checked again before the conversation is read: what
/// went stays kept, whether or not they may still read it.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    line: Option<&str>,
    reference: &Reference,
    written: &str,
    adding: (&[String], Option<&MessageGif>),
) -> Result<SentMessage> {
    let message = match reference {
        Reference::Slack(id) => {
            own(line)?;
            reply::send(state, c, access, id, written, adding).await?
        }
        Reference::Text(id) => {
            texts::reply::send(state, c, access, line, id, written, adding).await?
        }
    };
    let (shared, asked, sent_in, sent) = (
        state.clone(),
        c.clone(),
        reference.clone(),
        message.id.clone(),
    );
    let line = line.map(str::to_owned);
    let conversation = state
        .read(move |db| {
            let c = access.revalidate(db, &asked)?;
            match &sent_in {
                Reference::Slack(id) => conversations::one(db, &shared, &c.dsp.id, id),
                Reference::Text(id) => {
                    texts::reading::filed(db, &shared, &c.dsp.id, line.as_deref(), &sent, id)
                }
            }
        })
        .await?;
    Ok(SentMessage {
        conversation,
        message,
    })
}

#[cfg(test)]
#[path = "../tests/backend/dispatcher.rs"]
mod tests;
