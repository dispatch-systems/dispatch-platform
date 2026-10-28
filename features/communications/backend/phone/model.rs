//! What the phone side speaks in: the DSP's dispatch phones, the texts they relay and Dispatch's
//! sent from them, their pictures and files, and what one text may carry. Answering queues a text
//! here (`Outgoing`); filing reads the same text's row, and its files as they're here (`File`).
//!
//! Its IDs, of a phone, a text or a file, are opaque: at most `ID_MOST` ASCII letters, digits, `_`
//! or `-` (`identifier`), meaning nothing but to the phone side, which gave them.
use serde::Serialize;

/// The most characters an ID the phone side gives may have.
pub const ID_MOST: usize = 64;
/// Why a text Dispatch queued failed when its phone was removed before it went: its `error`.
pub const PHONE_REMOVED: &str = "phones_removed";

/// One of the DSP's dispatch phones.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Phone {
    pub id: String,
    /// What someone at the DSP calls it, as Dispatch.
    pub name: String,
    /// Its own number.
    pub number: String,
    /// Whether it has checked in lately.
    pub online: bool,
    /// When it last checked in, epoch milliseconds; none before it ever has.
    pub last_seen_at: Option<i64>,
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
    /// Why it failed: as the phone said, or `PHONE_REMOVED` for one whose phone was removed
    /// before it went.
    pub error: Option<String>,
    pub files: Vec<File>,
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
    /// Whether its bytes are kept; a phone's come after its text.
    pub kept: bool,
}

/// A text to send.
#[derive(Clone, Debug)]
pub struct Outgoing {
    /// The phone to send it from; none for the one that last texted `to`, else the one seen
    /// last.
    pub phone: Option<String>,
    /// The number to send it to.
    pub to: String,
    /// What it says, at most `LIMITS.body` characters: empty only with a picture.
    pub body: String,
    /// Pictures `by` added for it, at most `LIMITS.files`.
    pub files: Vec<String>,
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
