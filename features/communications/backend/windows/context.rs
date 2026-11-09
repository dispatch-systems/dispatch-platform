//! Compact render facts: three indexed one-row probes per nonempty selection. No full run,
//! local day, attachments, GIF or message body is read to answer boundary context.
use super::super::{
    model::{Direction, Reference},
    texts::{self, numbers::Numbers},
};
use crate::api::types::{Message, MessageNeighbor, MessageRenderContext, MessageSenderIdentity};
use dispatch_core::{
    Result,
    db::{FromRow, Row, Store},
    foundation::clock,
};
use rusqlite::params;

struct NeighborRow {
    id: String,
    ts: String,
    at: i64,
    direction: Direction,
    sent_by: Option<String>,
    sender_name: Option<String>,
    phone_id: Option<String>,
    group_id: Option<String>,
    number: Option<String>,
}
impl FromRow for NeighborRow {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            ts: row.get("ts")?,
            at: row.get("at")?,
            direction: row.get("direction")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            phone_id: row.get("phone_id")?,
            group_id: row.get("group_id")?,
            number: row.get("number")?,
        })
    }
}
impl NeighborRow {
    fn shown(self, numbers: Option<&Numbers>) -> MessageNeighbor {
        // Match conversations::messages / texts::reading::messages exactly, without hydrating
        // a Message. Slack's sender is sent_by when present; text's phone sender is out-only.
        let sender = if self.number.is_some() && self.direction != Direction::Out {
            None
        } else if let Some(user) = self.sent_by {
            Some(MessageSenderIdentity {
                user_id: Some(user),
                phone_id: None,
                name: self.sender_name.unwrap_or_default(),
            })
        } else {
            self.phone_id.map(|phone| MessageSenderIdentity {
                user_id: None,
                name: numbers
                    .and_then(|numbers| numbers.phone(&phone))
                    .unwrap_or("Phone")
                    .to_owned(),
                phone_id: Some(phone),
            })
        };
        MessageNeighbor {
            id: self.id,
            ts: self.ts,
            at: self.at,
            direction: self.direction,
            sender,
            group_id: self.group_id,
            number: self.number,
        }
    }
}

/// `messages` is the selected slice, never candidate rows or local receipts. The caller owns
/// the DSP transaction and shares the same Numbers snapshot as its text message hydration.
pub(crate) fn read(
    db: &Store,
    dsp: &str,
    reference: &Reference,
    messages: &[Message],
    numbers: Option<&Numbers>,
) -> Result<MessageRenderContext> {
    let (Some(first), Some(last)) = (messages.first(), messages.last()) else {
        return Ok(MessageRenderContext::default());
    };
    let connection = db.dsp(dsp)?;
    let probe = |message: &Message,
                 after: bool,
                 outgoing: bool|
     -> Result<Option<MessageNeighbor>> {
        // These fragments are fixed product query choices, never caller-provided SQL.
        let (compare, order) = if after { (">", "ASC") } else { ("<", "DESC") };
        let outgoing = if outgoing {
            " AND m.direction='out'"
        } else {
            ""
        };
        let row: Option<NeighborRow> = match reference {
            Reference::Slack(id) => connection.one_as(
                &format!("SELECT m.id,m.ts,m.at,m.direction,m.sent_by,m.sender_name, NULL AS phone_id,CASE WHEN \
                    m.direction='out' THEN g.id END AS group_id, NULL AS number FROM communications_messages \
                    m LEFT JOIN communications_groups g ON g.id=m.group_id WHERE m.conversation_id=?1 AND \
                    m.ts{compare}?2{outgoing} ORDER BY m.ts {order} LIMIT 1"),
                params![id, message.ts],
            )?,
            Reference::Text(id) => {
                let (at, text_id) = texts::reading::before(&message.ts)?;
                connection.one_as(
                    &format!("SELECT m.id,printf('%013d',m.at)||':'||m.id AS ts,m.at,m.direction, \
                        m.sent_by,m.sender_name,m.device_id AS phone_id,NULL AS group_id,m.number FROM \
                        phones_texts m WHERE m.conversation_id=?1 AND (m.at,m.id){compare}(?2,?3) {outgoing} \
                        ORDER BY m.at {order},m.id {order} LIMIT 1"),
                    params![id, at, text_id],
                )?
            }
        };
        Ok(row.map(|row| row.shown(numbers)))
    };
    let previous = probe(first, false, false)?;
    let next = probe(last, true, false)?;
    let zone = texts::zone(db, dsp)?;
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
