//! Queuing a text for a phone to send: one Dispatch sends, or one that failed, again. Each runs
//! inside the write its caller opens, so whatever it checks there of where the text goes, and of
//! who sends it, holds as the text is queued. Once that write is done, `live::queued` wakes the
//! phone.
use super::{
    model::{Outgoing, Text},
    storage::{DeviceStore, TextStore},
};
use dispatch_core::{Result, db::Store};

/// Queues `outgoing` from the phone it names, else the one that last texted its number, else the
/// one seen last: the text as accepted, waiting until its phone says how it went. The demo's
/// stand-in phone, which no app holds, says it delivered at once. Inside a write.
pub fn send(db: &Store, dsp: &str, outgoing: &Outgoing, at: i64) -> Result<Text> {
    let text = db.phones_send(dsp, outgoing, at)?;
    if !db.phones_stand_in(dsp, &text.phone)? {
        return Ok(text);
    }
    db.phones_stand_in_delivered(dsp, &text.id, at)?;
    Ok(db.phones_text(dsp, &text.id)?.unwrap_or(text))
}

/// Queues again the text `id`, one Dispatch sent that failed, as a new attempt from the phone it
/// went from, while that phone is still paired: waiting again, as `send`'s is. Inside a write.
pub fn retry(db: &Store, dsp: &str, id: &str, at: i64) -> Result<Text> {
    db.phones_retry(dsp, id, at)
}

#[cfg(test)]
#[path = "../../tests/backend/phone/outbox.rs"]
mod tests;
