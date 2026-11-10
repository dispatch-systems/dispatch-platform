//! Shared conversation and attachment models for Communications' transports.
use dispatch_core::{Error, Result, db, text_enum};

/// What a Slack conversation's ID begins with, before `_` and 32 hexadecimal digits.
pub const SLACK_PREFIX: &str = "cv";
/// What a conversation by text's ID begins with, as a Slack one's does.
pub const TEXT_PREFIX: &str = "tc";

/// How many answered conversations, or messages, are read at a time, of every kind.
pub const PAGE: usize = 50;

/// The most server messages in one authoritative conversation window.
pub const WINDOW: usize = 150;

/// The time Slack's `ts` names, in milliseconds: seconds, a point, then microseconds. None for
/// one that isn't, or is past what milliseconds hold.
pub fn millis(ts: &str) -> Option<i64> {
    let (seconds, micros) = ts.split_once('.').unwrap_or((ts, "0"));
    let micros = format!("{micros:0<6}");
    seconds
        .parse::<i64>()
        .ok()?
        .checked_mul(1000)?
        .checked_add(micros.get(..3)?.parse::<i64>().ok()?)
}
/// Slack's `ts` for the time `at`, in milliseconds.
pub fn ts(at: i64) -> String {
    format!("{}.{:03}000", at.div_euclid(1000), at.rem_euclid(1000))
}

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// What a conversation is: a Slack direct message, the texts of the work phones a driver
    /// had, each day's their latest that day (or of one nobody had then), or a number's texts.
    pub enum ConversationKind {
        Slack => "slack",
        Work => "work",
        Number => "number",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Whether a conversation waits on an answer: the member's latest message came after
    /// anyone at the DSP answered, or marked it done; someone at the DSP answered last; or
    /// marked it done without answering.
    pub enum ConversationState {
        Waiting => "waiting",
        Responded => "responded",
        Done => "done",
    }
}
text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Who wrote a message: the member (`in`), or someone at the DSP (`out`).
    pub enum Direction {
        In => "in",
        Out => "out",
    }
}

dispatch_core::text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// Where a file stands: being downloaded from Slack, or sent by a phone; kept in Dispatch;
    /// too large to keep; not one Slack hosts, as a link to elsewhere; or given up after failing.
    /// A text's video may be kept on its phone instead, until someone plays it, or be gone from
    /// it; Slack's never are.
    pub enum FileStatus {
        Waiting => "waiting",
        Kept => "kept",
        TooLarge => "too_large",
        Unavailable => "unavailable",
        Failed => "failed",
        OnPhone => "on_phone",
        Gone => "gone",
    }
}

/// One of the DSP's conversations, by the transport it's kept by and its ID there: a Slack
/// direct message, or texts, a driver's, a work phone's nobody held, or a number's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reference {
    Slack(String),
    Text(String),
}
impl Reference {
    /// The conversation `id` names, by what it begins with: refused for one that names none.
    pub fn parse(id: &str) -> Result<Self> {
        if db::identifier(id, &format!("{SLACK_PREFIX}_")) {
            return Ok(Self::Slack(id.to_owned()));
        }
        if db::identifier(id, &format!("{TEXT_PREFIX}_")) {
            return Ok(Self::Text(id.to_owned()));
        }
        Err(Error::new("invalid_input", 400))
    }
    /// Its ID, as its transport keeps it.
    pub fn id(&self) -> &str {
        match self {
            Self::Slack(id) | Self::Text(id) => id,
        }
    }
    /// Its ID, for what only a conversation by text has, as a number to name or block: refused
    /// for a Slack one.
    pub fn texted(self) -> Result<String> {
        match self {
            Self::Text(id) => Ok(id),
            Self::Slack(_) => Err(Error::new("invalid_input", 400)),
        }
    }
}
