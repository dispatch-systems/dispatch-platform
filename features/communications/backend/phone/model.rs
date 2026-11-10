//! What the phone side speaks in: the DSP's dispatch phones, the texts they relay and Dispatch's
//! sent from them, their pictures and files, and what one text may carry. Answering queues a text
//! here (`Outgoing`); filing reads the same text's row, and its files as they're here (`File`).
//!
//! Its IDs, of a phone, a text or a file, are opaque: at most `ID_MOST` ASCII letters, digits, `_`
//! or `-` (`identifier`), meaning nothing but to the phone side, which gave them.
use crate::api::types::MessageGif;
use serde::{Deserialize, Serialize};

/// The most characters an ID the phone side gives may have.
pub const ID_MOST: usize = 64;
/// Why a text Dispatch queued failed when its phone was removed before it went: its `error`.
pub const PHONE_REMOVED: &str = "phones_removed";
/// Why a text Dispatch queued failed when it still waited at its expiry on its phone, offline: its
/// `error`.
pub const PHONE_OFFLINE: &str = "phones_offline";
/// How long a text Dispatch queued, or tried again, may wait for its phone to start it, in
/// milliseconds: past it, the phone never does.
pub const EXPIRY_MS: i64 = 10 * 60 * 1000;
/// How long the bytes of a video kept on its phone are kept once it sent them, in milliseconds, so
/// another who plays it soon after has them at once: let go of by the hourly upkeep after it.
pub const FETCHED_MS: i64 = 60 * 60 * 1000;
/// How long someone playing a video kept on its phone waits for its bytes, in milliseconds: playing
/// it again within it while its phone sends them waits on the same, rather than asking again.
pub const FETCH_WAIT_MS: i64 = 40 * 1000;
/// How long a phone may still be told of a video someone asked for, in milliseconds: one told of it
/// later doesn't send it.
pub const WANT_MS: i64 = 2 * 60 * 1000;

/// How a phone says it stands, with each wait on its outbox: its Set up, texting and SIM, what
/// waits on it to go, and what last went wrong. Kept as it last said it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    /// The Set up steps it has undone, by the app's own keys.
    pub setup: Vec<String>,
    /// Whether Android lets it read the phone's texts, and send them.
    pub read: bool,
    pub send: bool,
    pub sim: Sim,
    /// What waits on it to go to Dispatch.
    pub waiting: Backlog,
    /// The picture messages received that the phone's texting app hasn't downloaded: none while it
    /// can't count them, as while Android doesn't let it read texts.
    pub undownloaded: Option<i64>,
    /// The phone's default texting app, by its package, where it has one.
    pub texting_app: Option<String>,
    /// Why a text last failed to send, as the phone said it to Dispatch, and when.
    pub send_error: Option<Happened>,
    /// The last error the app didn't expect: its class, and what it was doing, never what it held.
    pub unexpected: Option<Happened>,
}
impl Health {
    /// Whether it can't send texts: Android doesn't let it, or it has no SIM.
    pub fn cant_send(&self) -> bool {
        !self.send || self.sim == Sim::Absent
    }
}

dispatch_core::text_enum! {
    /// The SIM a phone texts from, as Android says it: in the phone and with the network, not in
    /// it, or in it with no network to text on.
    pub enum Sim {
        Ready => "ready",
        Absent => "absent",
        NoSignal => "no_signal",
    }
}

/// How many of each wait on a phone to go to Dispatch: the texts it read, their pictures' bytes,
/// and how Dispatch's texts went.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backlog {
    pub texts: i64,
    pub files: i64,
    pub statuses: i64,
}

/// Something that went wrong on a phone: what (`error`), as it was doing what (`doing`) where it
/// says, and when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Happened {
    pub error: String,
    #[serde(rename = "where", default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
    pub at: i64,
}

/// What one text may carry: queuing refuses more, and keeping a picture one past `largest`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most characters its body may have.
    pub body: usize,
    /// The most pictures or files it may carry.
    pub files: usize,
    /// The most bytes each may be.
    pub largest: u64,
}

/// A text, as a phone reported it or Dispatch sent it.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub id: String,
    /// The phone it went through.
    pub phone: String,
    /// `In`, to the DSP's number, or `Out`, from it.
    pub direction: Direction,
    /// Whether the phone reported it or Dispatch sent it.
    pub origin: Origin,
    /// The other party's number, as `numbers::normal` writes it.
    pub number: String,
    pub body: String,
    /// When it was written, epoch milliseconds.
    pub at: i64,
    /// Who at the DSP sent it, for one Dispatch sent.
    pub sent_by: Option<String>,
    /// How one Dispatch sent went; none for one the phone reported.
    pub status: Option<Status>,
    pub status_at: Option<i64>,
    /// Why it failed: as the phone said, `PHONE_REMOVED` for one whose phone was removed before
    /// it went, or `PHONE_OFFLINE` for one still waiting at its expiry on its phone, offline.
    pub error: Option<String>,
    pub files: Vec<File>,
    /// The GIF one Dispatch sent carries in place of pictures, if any: the phone fetches it from
    /// KLIPY as it sends the text.
    pub gif: Option<MessageGif>,
    /// Its latest change, after every one before it: a text changes as it comes, as Dispatch
    /// queues it or tries it again, as how it went changes, and as its files' bytes arrive.
    pub change: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

/// Who a text came from: the phone, received there or sent from its own messaging app, or
/// Dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Phone,
    Dispatch,
}

/// How a text Dispatch sent went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The phone hasn't sent it yet.
    Waiting,
    Sent,
    Delivered,
    Failed,
}

/// A text's picture or file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct File {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    /// Whether its bytes are kept; a phone's come after its text, and a video kept on its phone's
    /// once someone plays it, for an hour (`FETCHED_MS`).
    pub kept: bool,
    /// Its bytes' SHA-256, once they're kept.
    #[serde(skip)]
    pub sha256: Option<String>,
    /// Whether it's a video its phone keeps, sending its bytes only once someone plays it.
    pub on_phone: bool,
    /// Whether a picture of its first frame is kept, for a video kept on its phone.
    pub preview: bool,
    /// Whether its phone said it has it no more, for a video kept on it.
    pub gone: bool,
}

/// A text to send.
#[derive(Clone, Debug)]
pub struct Outgoing {
    /// The number to send it to.
    pub to: String,
    /// What it says, at most `LIMITS.body` characters: empty only with a picture.
    pub body: String,
    /// Pictures `by` added for it, at most `LIMITS.files`.
    pub files: Vec<String>,
    /// A GIF in place of pictures, if any.
    pub gif: Option<MessageGif>,
    /// Who at the DSP sends it.
    pub by: String,
}

/// A picture someone adds for a text not sent yet.
#[derive(Clone, Debug)]
pub struct Added {
    pub by: String,
    pub name: String,
    pub mime: String,
}

/// Whether `id` may be an ID the phone side gives a phone, a text or a file: 1 to `ID_MOST` ASCII
/// letters, digits, `_` or `-`. It says nothing more of it: only the phone side knows what it
/// names.
pub fn identifier(id: &str) -> bool {
    (1..=ID_MOST).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
#[path = "../../tests/backend/phone/identifiers.rs"]
mod tests;
