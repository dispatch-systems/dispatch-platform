//! Answering by text, with pictures, and sending again one that failed. An answer goes as
//! written, with nothing added, so the other party sees only the DSP's number: from the phone that
//! last texted that number, else the one seen last; and is kept as filing reads it back, with how
//! it went, under the full name of whoever wrote it, as a Slack answer is (`sent_by`).
//! A number's conversation, and a work phone's nobody had, answer their number; a driver's, the
//! work phone they have now, as live route data says (`LiveDay::phones`), and none while they have
//! none. Its pictures
//! are kept by the phone side as they're added, its adder's alone until they go with it, and none
//! of one kept should the adder's access end while its bytes come. One that failed is sent again
//! with Retry. Either is queued in the write that checks where it goes and that the member may
//! still send it, so no handoff, stop or change of their access comes between.
use super::{
    filing, live_day::LiveDay, numbers::work_number, reading::message, storage::TextStore,
};
use crate::{
    api::types::{CommunicationsUpload, Message},
    backend::{
        compose::{self, checked, photo_kind},
        live,
        model::ConversationKind,
        phone::{
            self, files,
            model::{Added, Outgoing},
            outbox,
        },
        storage::DOMAIN,
    },
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Store, now},
    ensure,
    server::http::{route::Dsp, upload::Upload},
};
use std::sync::Arc;

/// Sends what the member `c` wrote in the conversation by text `id`, with the pictures `photos`
/// they added for it, to where a text written there goes as it's queued: the text, as kept.
pub async fn send(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    written: &str,
    photos: &[String],
) -> Result<Message> {
    let body = checked(written, photos, compose::TEXT)?.to_owned();
    let (asked, at, files) = (c.clone(), id.to_owned(), photos.to_vec());
    let sent = state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &asked)?;
            let outgoing = Outgoing {
                phone: None,
                to: recipient(db, &c.dsp.id, &at)?,
                body,
                files,
                by: c.auth.user.id.clone(),
            };
            outbox::send(db, &c.dsp.id, &outgoing, now())
        })
        .await?;
    phone::live::queued(state, &c.dsp.id);
    live::stopped(state, &c.dsp.id, id, &c.auth.user.id);
    filing::read_on(state, &c.dsp.id).await?;
    let (shared, dsp) = (state.clone(), c.dsp.id.clone());
    state
        .read(move |db| message(db, &shared, &dsp, &sent.id))
        .await
}

/// Where a text written in the conversation `id` goes: a number's own number, or a work phone's
/// nobody had; for a driver's, the work phone they have now, refused while they have none.
fn recipient(db: &Store, dsp: &str, id: &str) -> Result<String> {
    let conversation = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let phone = match (&conversation.driver, conversation.kind) {
        (Some((source, driver)), _) => match LiveDay::read(db, dsp)? {
            Some(live) if live.source == *source => live.phones(db, dsp)?.remove(driver),
            _ => None,
        },
        (None, ConversationKind::Slack) => None,
        (None, _) => {
            return conversation
                .number
                .ok_or_else(|| Error::new("invalid_input", 400));
        }
    };
    phone
        .map(|phone| work_number(&phone))
        .ok_or_else(|| Error::new("communications_no_work_phone", 409))
}

/// Refuses the text `text` unless it's the conversation `id`'s and goes where a text written there
/// now would: not to a work phone its driver no longer has, which may be another's since.
fn goes_now(db: &Store, dsp: &str, id: &str, text: &str) -> Result<()> {
    let text = db
        .communications_text(dsp, text)?
        .filter(|text| text.conversation == id)
        .ok_or_else(|| Error::new("communications_message_not_found", 404))?;
    ensure(
        recipient(db, dsp, id)? == text.number,
        "communications_work_phone_changed",
        409,
    )
}

/// Sends again the text `text`, which Dispatch sent in the conversation `id` and failed, while it
/// goes where a text written now would, and the member may still send it, checked in the write
/// that queues it. The text, as kept.
pub async fn retry(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    id: &str,
    text: &str,
) -> Result<Message> {
    let (asked, at, wanted) = (c.clone(), id.to_owned(), text.to_owned());
    let sent = state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &asked)?;
            goes_now(db, &c.dsp.id, &at, &wanted)?;
            outbox::retry(db, &c.dsp.id, &wanted, now())
        })
        .await?;
    phone::live::queued(state, &c.dsp.id);
    filing::read_on(state, &c.dsp.id).await?;
    let (shared, dsp) = (state.clone(), c.dsp.id.clone());
    state
        .read(move |db| message(db, &shared, &dsp, &sent.id))
        .await
}

/// Keeps a picture the member `c` adds for a text not sent yet, named `name`, of the kind `mime`,
/// as the page reads it. One that isn't a photo a text carries is refused, by its kind. Their
/// access is checked again in the write that keeps it, once its bytes are in, as Slack's photos'
/// is: one whose access ended meanwhile keeps none of it.
pub async fn upload(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    name: &str,
    mime: &str,
    photo: Upload,
) -> Result<CommunicationsUpload> {
    ensure(photo_kind(mime).is_some(), "communications_photo_kind", 415)?;
    let arrived = files::receive(state, &c.dsp.id, photo).await?;
    let asked = c.clone();
    let added = Added {
        by: c.auth.user.id.clone(),
        name: name.to_owned(),
        mime: mime.to_owned(),
    };
    let kept = state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            let c = access.revalidate(db, &asked)?;
            files::keep_added(db, &c.dsp.id, added, &arrived)
        })
        .await?;
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
