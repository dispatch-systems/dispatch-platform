//! What texting keeps, as its storage reads and writes it: each conversation by text, each text
//! filed in one with its pictures and how one Dispatch sent went, and where a text is filed
//! (`Filed`).
use crate::backend::{
    model::{ConversationKind, ConversationState, Direction},
    phone::model::File,
};
use dispatch_core::{
    Result,
    db::{FromRow, Row},
    text_enum,
};

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// How a text Dispatch sent went: waiting on the phone to send it, sent, delivered, or
    /// failed.
    pub enum TextStatus {
        Waiting => "waiting",
        Sent => "sent",
        Delivered => "delivered",
        Failed => "failed",
    }
}

/// A conversation by text, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextConversation {
    pub id: String,
    pub kind: ConversationKind,
    /// For a driver's, the source of their ID and the ID.
    pub driver: Option<(String, String)>,
    /// For a number's, its number; for a work phone's nobody had, the phone's.
    pub number: Option<String>,
    pub state: ConversationState,
    pub waiting_since: Option<i64>,
    pub last_at: Option<i64>,
}
impl FromRow for TextConversation {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            kind: row.get("kind")?,
            driver: match (row.get("source")?, row.get("driver_id")?) {
                (Some(source), Some(id)) => Some((source, id)),
                _ => None,
            },
            number: row.get("number")?,
            state: row.get("state")?,
            waiting_since: row.get("waiting_since")?,
            last_at: row.get("last_at")?,
        })
    }
}

/// A text, as kept: the phone side's row of it, with what filing keeps beside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// The phone side's ID of it.
    pub id: String,
    /// The conversation it's filed in: none while it waits to be (`filing`).
    pub conversation: Option<String>,
    /// The DSP's phone it went through.
    pub phone: String,
    pub direction: Direction,
    /// The other party's number.
    pub number: String,
    pub body: String,
    pub at: i64,
    /// Who at the DSP sent it, for one Dispatch sent, and the name they're shown by.
    pub sent_by: Option<String>,
    pub sender_name: Option<String>,
    /// How one Dispatch sent went, and why it failed.
    pub status: Option<TextStatus>,
    pub error: Option<String>,
    /// Its pictures and files, as the phone side keeps them.
    pub files: Vec<File>,
}
/// A text from its row, its files read after (`files` empty).
impl FromRow for Text {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            conversation: row.get("conversation_id")?,
            phone: row.get("device_id")?,
            direction: row.get("direction")?,
            number: row.get("number")?,
            body: row.get("body")?,
            at: row.get("at")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            status: row.get("status")?,
            error: row.get("error")?,
            files: Vec::new(),
        })
    }
}

/// A text waiting to be filed, as placing it reads it: its number, when it was written, and who at
/// the DSP sent it, for one Dispatch sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unfiled {
    pub id: String,
    pub number: String,
    pub at: i64,
    pub sent_by: Option<String>,
}
impl FromRow for Unfiled {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            number: row.get("number")?,
            at: row.get("at")?,
            sent_by: row.get("sent_by")?,
        })
    }
}

/// Where a text is filed: under the driver whose latest work phone it was the day it was written,
/// by their ID of a source; under a work phone nobody had then; or under its number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Filed {
    Driver { source: String, id: String },
    WorkPhone(String),
    Number(String),
}
