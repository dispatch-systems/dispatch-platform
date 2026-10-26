//! Answering by text, and what someone at the DSP does with a number. An answer goes as written,
//! with nothing added, so the other party sees only the DSP's number: through `texting`, from
//! the phone that last texted that number, else the one seen last; and is kept as Phones then
//! tells of it, with how it went, under the full name of whoever wrote it, as a Slack answer is. One that failed is sent again with
//! Retry. Only a number's conversation is answered here: a driver's work phones come with Today's
//! Routes. The member's access is checked again before a text goes.
use super::{
    filing, message, normal, one,
    storage::{ConversationKind, TextConversation, TextStore},
};
use crate::{
    api::types::{Conversation, Message},
    backend::{live, reply::checked, sender, storage::DOMAIN},
};
use dispatch_core::{
    Error, Result, State, accounts::Context, db::Store, ensure, server::http::route::Dsp,
};
use dispatch_phones_contract::{self as texting, Outgoing, Texting};
use std::sync::Arc;

/// The most a contact's name may be, in characters.
pub const NAME_MOST: usize = 80;

/// Sends what the member `c` wrote in the number's conversation `id`, by text: the text, as kept.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
) -> Result<Message> {
    let body = checked(written, &[])?.to_owned();
    let (dsp, at) = (c.dsp.id.clone(), id.to_owned());
    let (number, texting) = state
        .read(move |db| {
            let number = numbered(&found(db, &dsp, &at)?)?;
            Ok((number, offering(db, &dsp)?))
        })
        .await?;
    // Checked again before it goes: once Phones has it, it's sent.
    let asked = c.clone();
    let c = state.read(move |db| access.revalidate(db, &asked)).await?;
    let outgoing = Outgoing {
        phone: None,
        to: number,
        body,
        files: Vec::new(),
        by: c.auth.user.id.clone(),
    };
    let sent = texting.send(state, &c.dsp.id, outgoing).await?;
    live::stopped(state, &c.dsp.id, id, &c.auth.user.id);
    filing::read_on(state, &c.dsp.id).await?;
    let (shared, dsp, name) = (state.clone(), c.dsp.id.clone(), sender::name(&c.auth.user));
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            db.name_communications_text(&dsp, &sent.id, &name)?;
            message(db, &shared, &dsp, &sent.id)
        })
        .await
}

/// Sends again the text `text`, which Dispatch sent in the conversation `id` and failed: the
/// text, as kept.
pub async fn retry(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    text: &str,
) -> Result<Message> {
    let (dsp, at, wanted) = (c.dsp.id.clone(), id.to_owned(), text.to_owned());
    let texting = state
        .read(move |db| {
            let text = db.communications_text(&dsp, &wanted)?;
            ensure(
                text.is_some_and(|text| text.conversation == at),
                "communications_message_not_found",
                404,
            )?;
            offering(db, &dsp)
        })
        .await?;
    let asked = c.clone();
    let c = state.read(move |db| access.revalidate(db, &asked)).await?;
    let sent = texting.retry(state, &c.dsp.id, text).await?;
    filing::read_on(state, &c.dsp.id).await?;
    let (shared, dsp) = (state.clone(), c.dsp.id.clone());
    state
        .read(move |db| message(db, &shared, &dsp, &sent.id))
        .await
}

/// The number `raw`'s conversation, opened with no text if it's new, for someone at the DSP to
/// text first.
pub fn open(db: &Store, state: &State, dsp: &str, raw: &str) -> Result<Conversation> {
    let number = normal(raw).ok_or_else(|| Error::new("invalid_input", 400))?;
    offering(db, dsp)?;
    let opened = db.open_communications_number(dsp, &number)?;
    one(db, state, dsp, &opened.id)
}

/// Saves `name` for the number of the conversation `id`, as the member `by`.
pub fn save_contact(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    name: &str,
    by: &str,
) -> Result<Conversation> {
    let number = numbered(&found(db, dsp, id)?)?;
    db.save_communications_contact(dsp, &number, name, by)?;
    live::changed(state, dsp);
    one(db, state, dsp, id)
}

/// Blocks the number of the conversation `id`, as the member `by`, or unblocks it with none.
pub fn block(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    by: Option<&str>,
) -> Result<Conversation> {
    let number = numbered(&found(db, dsp, id)?)?;
    db.block_communications_number(dsp, &number, by)?;
    live::changed(state, dsp);
    one(db, state, dsp, id)
}

/// The DSP's conversation by text `id`.
fn found(db: &Store, dsp: &str, id: &str) -> Result<TextConversation> {
    db.communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))
}
/// A number's conversation's number: none is answered here but a number's.
fn numbered(conversation: &TextConversation) -> Result<String> {
    match (&conversation.kind, &conversation.number) {
        (ConversationKind::Number, Some(number)) => Ok(number.clone()),
        _ => Err(Error::new("invalid_input", 400)),
    }
}
/// What offers the DSP texting, refused while nothing does.
fn offering(db: &Store, dsp: &str) -> Result<&'static dyn Texting> {
    texting::for_dsp(db, dsp)?.ok_or_else(|| Error::new("communications_texting_off", 409))
}
#[cfg(test)]
#[path = "../../tests/backend/texts/reply.rs"]
mod tests;
