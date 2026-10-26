//! Texting, as any feature uses it without depending on Phones: the texts and pictures a DSP's
//! dispatch phones relay, and sending one from the DSP's own number. Phones exposes it as
//! `texting`. Every call names the DSP and works only inside what Phones keeps for it.
//!
//! Its IDs, of a phone, a text or a file, are opaque: at most `ID_MOST` ASCII letters, digits, `_`
//! or `-` (`identifier`), meaning nothing but to whoever gave them.
use dispatch_core::{
    Result, State,
    db::Store,
    manifest::events::Event,
    server::http::{input::Reply, upload::Upload},
};
use serde::Serialize;
use std::{future::Future, pin::Pin, sync::Arc};

/// The most characters an ID texting gives may have.
pub const ID_MOST: usize = 64;

/// What its asynchronous answers are: work done outside the database, as keeping bytes is.
pub type Answer<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// What must hold for a text to go, checked in the write that queues it, so nothing it reads can
/// change before the text is queued: its refusal is the answer.
pub type Condition = Box<dyn FnOnce(&Store) -> Result<()> + Send>;

/// A DSP's texts by phone: those its phones receive and send, and those Dispatch sends from
/// them.
pub trait Texting: Send + Sync {
    /// The DSP's phones, those removed left out.
    fn phones(&self, db: &Store, dsp: &str) -> Result<Vec<Phone>>;
    /// Each text changed after `after`, a text's `change`, as it is now, oldest change first: a
    /// page of them, as many as texting gives at once, saying where to read on and whether more
    /// remain.
    fn changes(&self, db: &Store, dsp: &str, after: i64) -> Result<Changes>;
    /// Sends `outgoing` from one of the DSP's phones, provided `condition` holds as it's queued,
    /// waiting for it until it says how it went. Refused as `condition` refuses it, as
    /// `phones_none` while the DSP has no phone, `phones_not_found` for a phone not its own, and
    /// `phones_file_not_found` for a picture not added by `by`.
    fn send<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        outgoing: Outgoing,
        condition: Condition,
    ) -> Answer<'a, Text>;
    /// Sends again a text Dispatch sent that failed, provided `condition` holds as it's queued.
    /// Refused as `condition` refuses it, and as `phones_text_not_found` otherwise.
    fn retry<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        condition: Condition,
    ) -> Answer<'a, Text>;
    /// Keeps a picture for a text not sent yet, `by`'s alone until it's sent: its bytes as
    /// they arrive, refused before any past `limits().largest`.
    fn add_file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        added: Added,
        bytes: Upload,
    ) -> Answer<'a, File>;
    /// Lets go of a picture `by` added for a text not sent yet, and its bytes once no other file
    /// keeps them, as when `by` may no longer send it; nothing for one sent, another's, or one
    /// not the DSP's.
    fn remove_file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        by: &'a str,
    ) -> Answer<'a, ()>;
    /// What one text may carry, as `send` and `add_file` refuse more.
    fn limits(&self) -> Limits;
    /// A text's picture or file as kept, to answer a page with; none for one not the DSP's,
    /// one whose bytes haven't come yet, and one added for a text not sent yet but by `viewer`.
    fn file<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        id: &'a str,
        viewer: &'a str,
    ) -> Answer<'a, Option<Reply>>;
}

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

/// A page of the feed of each text as it is now: those changed after where a reader left off,
/// each once, oldest change first. A text changes as it comes, as Dispatch queues it or tries it
/// again, as how it went changes, and as its files' bytes arrive; the feed keeps no history of
/// the changes themselves.
#[derive(Clone, Debug, PartialEq)]
pub struct Changes {
    pub texts: Vec<Text>,
    /// Where reading on starts: after every change this page read.
    pub next: i64,
    /// Whether more changes remain after `next`, to read on for at once.
    pub more: bool,
}

/// What one text may carry: `send` refuses more, and `add_file` a picture past `largest`.
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
    /// The other party's number: `+` and its digits for one in North America.
    pub number: String,
    pub body: String,
    /// When it was written, epoch milliseconds.
    pub at: i64,
    /// Who at the DSP sent it, for one Dispatch sent.
    pub sent_by: Option<String>,
    /// How one Dispatch sent went; none for one the phone reported.
    pub status: Option<Status>,
    pub status_at: Option<i64>,
    /// Why it failed, as the phone said.
    pub error: Option<String>,
    pub files: Vec<File>,
    /// Its latest change, after every one before it.
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
#[derive(Clone, Debug, PartialEq, Serialize)]
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
    /// What it says, at most `limits().body` characters: empty only with a picture.
    pub body: String,
    /// Pictures `by` added for it, at most `limits().files`.
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

/// The DSP's texts changed: one came, was sent, or how one went changed. Read `changes` from
/// where you left off.
#[derive(Clone, Debug)]
pub struct TextsChanged;
impl Event for TextsChanged {
    const NAME: &'static str = "phones.texts_changed";
}

/// `raw` as texting writes a phone number, so one number is always written one way: `+1` and
/// ten digits for one in North America, `+` and its digits for one dialed with its country code,
/// and the digits alone for a short code, as a carrier's or a business's; none for something that
/// isn't a number. Each text's `number` is written so, and `Outgoing`'s `to` is read so.
pub fn normal(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 32 {
        return None;
    }
    let plus = raw.starts_with('+');
    if !raw
        .chars()
        .all(|c| c.is_ascii_digit() || " +-().".contains(c))
    {
        return None;
    }
    let bare = raw.chars().all(|c| c.is_ascii_digit());
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 if !plus => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        8..=15 if plus => Some(format!("+{digits}")),
        3..=7 if bare => Some(digits),
        _ => None,
    }
}

/// Whether `id` may be an ID texting gives a phone, a text or a file: 1 to `ID_MOST` ASCII
/// letters, digits, `_` or `-`. It says nothing more of it: only whoever gave it knows what it
/// names.
pub fn identifier(id: &str) -> bool {
    (1..=ID_MOST).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// What offers it for the DSP: only while Phones is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn Texting>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn Texting>(db, dsp)
}
