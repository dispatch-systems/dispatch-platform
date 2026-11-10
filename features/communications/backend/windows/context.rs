//! Compact render facts: three indexed one-row probes per nonempty selection. No full run,
//! local day, attachments, GIF or message body is read to answer boundary context.
use super::super::{
    model::Reference,
    origins::{self, Place},
    texts::{self, numbers::Numbers},
};
use crate::api::types::{Message, MessageNeighbor, MessageRenderContext};
use dispatch_core::{Result, db::Store, foundation::clock};

/// `messages` is the selected slice, never candidate rows or local receipts. The caller owns
/// the DSP transaction and shares the same Numbers snapshot as its text message hydration.
/// `zone` is the DSP's where the caller holds it already; without one it is read here, and
/// only for a nonempty selection.
pub(crate) fn read(
    db: &Store,
    dsp: &str,
    reference: &Reference,
    messages: &[Message],
    numbers: Option<&Numbers>,
    zone: Option<chrono_tz::Tz>,
) -> Result<MessageRenderContext> {
    let (Some(first), Some(last)) = (messages.first(), messages.last()) else {
        return Ok(MessageRenderContext::default());
    };
    let zone = match zone {
        Some(zone) => zone,
        None => texts::zone(db, dsp)?,
    };
    let connection = db.dsp(dsp)?;
    // Where a message stands: a text's place is the one its `ts` names.
    let probe =
        |message: &Message, after: bool, outgoing: bool| -> Result<Option<MessageNeighbor>> {
            let (at, id) = match reference {
                Reference::Slack(_) => (message.at, message.id.clone()),
                Reference::Text(_) => texts::reading::before(&message.ts)?,
            };
            let place = Place {
                ts: &message.ts,
                at,
                id: &id,
            };
            origins::beside(&connection, reference, place, after, outgoing, numbers)
        };
    let previous = probe(first, false, false)?;
    let next = probe(last, true, false)?;
    // Nearest outgoing seek first, local-day comparison second. No derived local-date SQL
    // filter, UTC truncation, 24-hour subtraction, incoming-suffix scan or day/run walk.
    let sent_before = probe(first, false, true)?
        .filter(|row| clock::day(zone, row.at) == clock::day(zone, first.at))
        .and_then(|row| row.sender);
    Ok(MessageRenderContext {
        previous,
        next,
        sent_before,
    })
}
