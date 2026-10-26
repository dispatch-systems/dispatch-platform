//! Answering by text, with pictures, and sending again one that failed. An answer goes as
//! written, with nothing added, so the other party sees only the DSP's number: through `texting`,
//! from the phone that last texted that number, else the one seen last; and is kept as Phones then
//! tells of it, with how it went, under the full name of whoever wrote it, as a Slack answer is.
//! A number's conversation, and a work phone's nobody held, answer their number; a driver's, the
//! work phone they hold now, as live route data says, and none while they hold none. Its pictures
//! are kept by `texting` as they're added, its adder's alone until they go with it, and let go of
//! should the adder's access end while their bytes come. One that failed is sent again with Retry.
//! Either goes only where a text written now goes, checked as `texting` queues it, so no handoff or
//! stop comes between; and the member's access is checked again before it goes.
use super::{
    filing,
    live_day::LiveDay,
    numbers::work_number,
    offering,
    reading::message,
    storage::{ConversationKind, TextStore},
};
use crate::{
    api::types::{CommunicationsUpload, Message},
    backend::{
        live,
        reply::{PHOTO_KINDS, checked},
        sender,
        storage::DOMAIN,
    },
};
use dispatch_core::{
    Error, Result, State, accounts::Context, db::Store, ensure, server::http::route::Dsp,
    server::http::upload::Upload,
};
use dispatch_phones_contract::{Added, Outgoing};
use std::sync::Arc;

/// Sends what the member `c` wrote in the conversation by text `id`, with the pictures `photos`
/// they added for it, while it goes where a text written there now goes: the text, as kept.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
    photos: &[String],
) -> Result<Message> {
    let body = checked(written, photos)?.to_owned();
    let (dsp, at) = (c.dsp.id.clone(), id.to_owned());
    let (to, texting) = state
        .read(move |db| Ok((recipient(db, &dsp, &at)?, offering(db, &dsp)?)))
        .await?;
    // Checked again before it goes: once Phones has it, it's sent.
    let asked = c.clone();
    let c = state.read(move |db| access.revalidate(db, &asked)).await?;
    let (dsp, at, number) = (c.dsp.id.clone(), id.to_owned(), to.clone());
    let condition = Box::new(move |db: &Store| goes_to(db, &dsp, &at, &number));
    let outgoing = Outgoing {
        phone: None,
        to,
        body,
        files: photos.to_vec(),
        by: c.auth.user.id.clone(),
    };
    let sent = texting.send(state, &c.dsp.id, outgoing, condition).await?;
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

/// Where a text written in the conversation `id` goes: a number's own number, or a work phone's
/// nobody held; for a driver's, the work phone they hold now, refused while they hold none.
fn recipient(db: &Store, dsp: &str, id: &str) -> Result<String> {
    let conversation = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let held = match (&conversation.driver, conversation.kind) {
        (Some((source, driver)), _) => match LiveDay::read(db, dsp)? {
            Some(live) if live.source == *source => {
                live.held_by(db, dsp, std::slice::from_ref(driver))?
            }
            _ => None,
        },
        (None, ConversationKind::Slack) => None,
        (None, _) => {
            return conversation
                .number
                .ok_or_else(|| Error::new("invalid_input", 400));
        }
    };
    held.map(|held| work_number(&held.phone))
        .ok_or_else(|| Error::new("communications_no_work_phone", 409))
}

/// Refuses a text to `number` unless it's where a text written in the conversation `id` goes
/// now: not a work phone its driver no longer holds, which may be another's since.
fn goes_to(db: &Store, dsp: &str, id: &str, number: &str) -> Result<()> {
    ensure(
        recipient(db, dsp, id)? == number,
        "communications_work_phone_changed",
        409,
    )
}

/// Refuses the text `text` unless it's the conversation `id`'s and goes where a text written there
/// now would.
fn goes_now(db: &Store, dsp: &str, id: &str, text: &str) -> Result<()> {
    let text = db
        .communications_text(dsp, text)?
        .filter(|text| text.conversation == id)
        .ok_or_else(|| Error::new("communications_message_not_found", 404))?;
    goes_to(db, dsp, id, &text.number)
}

/// Sends again the text `text`, which Dispatch sent in the conversation `id` and failed, while it
/// goes where a text written now would: not to a work phone the driver no longer holds, which may
/// be another's since. Checked as `texting` queues it, so no handoff or stop comes between. The
/// text, as kept.
pub async fn retry(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    text: &str,
) -> Result<Message> {
    let dsp = c.dsp.id.clone();
    let texting = state.read(move |db| offering(db, &dsp)).await?;
    let asked = c.clone();
    let c = state.read(move |db| access.revalidate(db, &asked)).await?;
    let (dsp, at, wanted) = (c.dsp.id.clone(), id.to_owned(), text.to_owned());
    let condition = Box::new(move |db: &Store| goes_now(db, &dsp, &at, &wanted));
    let sent = texting.retry(state, &c.dsp.id, text, condition).await?;
    filing::read_on(state, &c.dsp.id).await?;
    let (shared, dsp) = (state.clone(), c.dsp.id.clone());
    state
        .read(move |db| message(db, &shared, &dsp, &sent.id))
        .await
}

/// Keeps a picture the member `c` adds for a text not sent yet, named `name`, of the kind `mime`,
/// through `texting`: as the page reads it. One that isn't a photo a text carries is refused, by
/// its kind. Their access is checked again once its bytes are in, as Slack's photos' is: one whose
/// access ended meanwhile keeps none of it.
pub async fn upload(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    name: &str,
    mime: &str,
    photo: Upload,
) -> Result<CommunicationsUpload> {
    ensure(
        PHOTO_KINDS.iter().any(|(kind, _)| *kind == mime),
        "communications_photo_kind",
        415,
    )?;
    let dsp = c.dsp.id.clone();
    let texting = state.read(move |db| offering(db, &dsp)).await?;
    let added = Added {
        by: c.auth.user.id.clone(),
        name: name.to_owned(),
        mime: mime.to_owned(),
    };
    let kept = texting.add_file(state, &c.dsp.id, added, photo).await?;
    let asked = c.clone();
    if let Err(refused) = state.read(move |db| access.revalidate(db, &asked)).await {
        texting
            .remove_file(state, &c.dsp.id, &kept.id, &c.auth.user.id)
            .await?;
        return Err(refused);
    }
    Ok(CommunicationsUpload {
        id: kept.id,
        name: kept.name,
        mime: kept.mime,
        size: kept.size,
    })
}

#[cfg(test)]
#[path = "../../tests/backend/texts/reply.rs"]
mod tests;
