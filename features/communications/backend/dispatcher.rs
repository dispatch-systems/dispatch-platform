//! One conversation of either transport, by its reference (`model::Reference`): reading it, the
//! profile beside it, sending in it, marking it Done and saying who's writing in it each go
//! through here, to Slack's (`conversations`, `reply`) or texting's (`texts`). What only one
//! transport has, as a number's contact or Retry, goes to it directly. A new transport adds its arm
//! to each.
use super::{
    conversations, live,
    model::Reference,
    profile, reply,
    storage::CommunicationsStore,
    texts::{self, storage::TextStore},
};
use crate::api::types::{Conversation, ConversationMessages, Message, Profile};
use dispatch_core::{
    Result, State, accounts::Context, db::Store, ensure, server::http::route::Dsp,
};
use std::sync::Arc;

/// The conversation `reference` and its latest messages, or those before the one at `before`,
/// as each transport places its messages.
pub fn opened(
    db: &Store,
    state: &State,
    dsp: &str,
    reference: &Reference,
    before: Option<&str>,
) -> Result<ConversationMessages> {
    match reference {
        Reference::Slack(id) => {
            let before = before.map(conversations::before).transpose()?;
            conversations::opened(db, state, dsp, id, before)
        }
        Reference::Text(id) => {
            let before = before.map(texts::reading::before).transpose()?;
            let before = before.as_ref().map(|(at, id)| (*at, id.as_str()));
            texts::reading::opened(db, state, dsp, id, before)
        }
    }
}

/// What the profile beside the conversation `reference` shows of whom it's with.
pub fn profile(db: &Store, state: &State, dsp: &str, reference: &Reference) -> Result<Profile> {
    match reference {
        Reference::Slack(id) => profile::member(db, state, dsp, id),
        Reference::Text(id) => profile::driver(db, state, dsp, id),
    }
}

/// Marks the conversation `reference` done without an answer, as the member `by`, while it
/// waits: the conversation, as the page reads it.
pub fn done(
    db: &Store,
    state: &State,
    dsp: &str,
    reference: &Reference,
    by: &str,
) -> Result<Conversation> {
    found(db, dsp, reference)?;
    let marked = match reference {
        Reference::Slack(id) => db.mark_communications_done(dsp, id, by)?,
        Reference::Text(id) => db.mark_communications_text_done(dsp, id, by)?,
    };
    if marked {
        live::changed(state, dsp);
    }
    one(db, state, dsp, reference)
}

/// The conversation `reference`, as the page reads it.
pub fn one(db: &Store, state: &State, dsp: &str, reference: &Reference) -> Result<Conversation> {
    match reference {
        Reference::Slack(id) => conversations::one(db, state, dsp, id),
        Reference::Text(id) => texts::reading::one(db, state, dsp, id),
    }
}

/// Refuses the conversation `reference` unless it's the DSP's.
pub fn found(db: &Store, dsp: &str, reference: &Reference) -> Result<()> {
    let found = match reference {
        Reference::Slack(id) => db.communications_conversation(dsp, id)?.is_some(),
        Reference::Text(id) => db.communications_text_conversation(dsp, id)?.is_some(),
    };
    ensure(found, "communications_conversation_not_found", 404)
}

/// Sends what the member `c` wrote in the conversation `reference`, with the photos `photos` they
/// added for it: the message, as the page reads it.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    reference: &Reference,
    written: &str,
    photos: &[String],
) -> Result<Message> {
    match reference {
        Reference::Slack(id) => reply::send(state, c, access, id, written, photos).await,
        Reference::Text(id) => texts::reply::send(state, c, access, id, written, photos).await,
    }
}
