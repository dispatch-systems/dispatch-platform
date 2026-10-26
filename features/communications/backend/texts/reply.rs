//! Answering by text, and sending again one that failed. An answer goes as written, with nothing
//! added, so the other party sees only the DSP's number: through `texting`, from the phone that
//! last texted that number, else the one seen last; and is kept as Phones then tells of it, with
//! how it went, under the full name of whoever wrote it, as a Slack answer is. One that failed is
//! sent again with Retry. Only a number's conversation is answered here: a driver's work phones
//! come with Today's Routes. The member's access is checked again before a text goes.
use super::{
    filing, offering,
    reading::{message, numbered},
    storage::TextStore,
};
use crate::{
    api::types::Message,
    backend::{live, reply::checked, sender, storage::DOMAIN},
};
use dispatch_core::{Result, State, accounts::Context, ensure, server::http::route::Dsp};
use dispatch_phones_contract::Outgoing;
use std::sync::Arc;

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
            let number = numbered(db, &dsp, &at)?;
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

#[cfg(test)]
#[path = "../../tests/backend/texts/reply.rs"]
mod tests;
