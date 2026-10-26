//! Pick driver: someone at the DSP files a work phone nobody held, as live route data shows it,
//! under one of today's drivers, for the DSP's day. Its texts of today are filed again, under the
//! driver picked but where a hold covers one (`filing`), and stay so as the day's holds settle.
use super::{
    filing,
    live_day::LiveDay,
    numbers::ten_digits,
    reading,
    storage::{ConversationKind, Filed, TextStore},
};
use crate::{api::types::Conversation, backend::live};
use dispatch_core::{Error, Result, State, db::Store, ensure};

/// Picks `driver`, one of today's drivers, as holding the work phone of the conversation `id`
/// today, as the member `by`: the driver's conversation, where the phone's texts of today are now.
pub fn pick(
    db: &Store,
    state: &State,
    dsp: &str,
    id: &str,
    driver: &str,
    by: &str,
) -> Result<Conversation> {
    let conversation = db
        .communications_text_conversation(dsp, id)?
        .ok_or_else(|| Error::new("communications_conversation_not_found", 404))?;
    let phone = match (conversation.kind, conversation.driver, conversation.number) {
        (ConversationKind::Work, None, Some(number)) => ten_digits(&number),
        _ => None,
    }
    .ok_or_else(|| Error::new("invalid_input", 400))?;
    let live =
        LiveDay::read(db, dsp)?.ok_or_else(|| Error::new("communications_routes_over", 409))?;
    ensure(
        live.drives(db, dsp, driver)?,
        "communications_driver_not_found",
        404,
    )?;
    let picked = Filed::Driver {
        source: live.source,
        id: driver.to_owned(),
    };
    db.pick_communications_driver(dsp, &phone, &live.day, &picked, by)?;
    filing::refiled(db, dsp)?;
    let theirs = db.open_communications_text(dsp, &picked)?;
    live::changed(state, dsp);
    reading::one(db, state, dsp, &theirs.id)
}

#[cfg(test)]
#[path = "../../tests/backend/texts/picking.rs"]
mod tests;
