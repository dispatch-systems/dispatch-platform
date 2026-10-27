//! What texting keeps, as its storage reads and writes it: each conversation by text, each text
//! filed in one with its pictures and how one Dispatch sent went, and where a text is filed
//! (`Filed`).
use crate::backend::model::{ConversationKind, ConversationState, Direction};
use dispatch_core::{
    Result,
    db::{FromRow, Row},
    text_enum,
};
use serde::{Deserialize, Serialize};

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
    /// For a number's, its number; for a work phone's nobody held, the phone's.
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

/// A text, as kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// Phones' ID of it.
    pub id: String,
    pub conversation: String,
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
    pub files: Vec<TextFile>,
}
impl FromRow for Text {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        let files: String = row.get("files")?;
        Ok(Self {
            id: row.get("id")?,
            conversation: row.get("conversation_id")?,
            phone: row.get("phone")?,
            direction: row.get("direction")?,
            number: row.get("number")?,
            body: row.get("body")?,
            at: row.get("at")?,
            sent_by: row.get("sent_by")?,
            sender_name: row.get("sender_name")?,
            status: row.get("status")?,
            error: row.get("error")?,
            files: serde_json::from_str(&files)?,
        })
    }
}
/// A text's picture or file, as Phones has it: whether its bytes are kept yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub kept: bool,
}

/// Where a text is filed: under the driver who held the work phone it was written with, by
/// their ID of a source; under a work phone nobody held then; or under its number.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Filed {
    Driver { source: String, id: String },
    WorkPhone(String),
    Number(String),
}

/// A text to keep, as Phones has it now, and where it's filed, if it's new.
pub struct Incoming {
    pub id: String,
    pub phone: String,
    pub direction: Direction,
    pub number: String,
    pub body: String,
    pub at: i64,
    /// Who at the DSP sent it, for one Dispatch sent, and the name they're shown by as it's
    /// first kept.
    pub sent_by: Option<String>,
    pub sender_name: Option<String>,
    pub status: Option<TextStatus>,
    pub error: Option<String>,
    pub files: Vec<TextFile>,
    pub filed: Filed,
}
