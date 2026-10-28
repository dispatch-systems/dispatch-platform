//! What a conversation of either kind is answered with: the conversation, whom it's with, its
//! messages, whom it's with's other conversations, the profile beside it, whom the DSP can
//! message, and a photo added for a message.
use super::{
    ConversationKind, ConversationState, Direction, FileStatus, Group, Pace, TextStatus, Workday,
};
use dispatch_core::manifest::people::{PersonRole, PersonTitle};
use serde::Serialize;

/// A conversation: a direct message with a member of one of the DSP's Slack workspaces, or texts
/// with a number, or with whichever of the DSP's work phones a driver held.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub kind: ConversationKind,
    /// The workspace it's in, for a Slack one.
    pub team_id: Option<String>,
    /// The number it texts: a number's own, a work phone's nobody held, or for a driver's, the
    /// work phone last texted with.
    pub number: Option<String>,
    /// For a driver's work conversation, their ID of the source naming who held the DSP's work
    /// phones.
    pub driver_id: Option<String>,
    /// Whether its number is one the DSP saved a name for.
    pub contact: bool,
    /// Whether its number is blocked: it isn't listed or counted.
    pub blocked: bool,
    /// Who it's with.
    pub with: Correspondent,
    pub state: ConversationState,
    /// Since when it waits on an answer.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since: Option<i64>,
    /// When its latest message was written; none before its first.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_at: Option<i64>,
    /// Its latest message, if it has one.
    pub last: Option<Message>,
    /// The letters of its name that matched what was searched, as runs of the name's UTF-16
    /// code units: start and end. None unless searched.
    pub matched: Vec<(u32, u32)>,
    /// The group it's listed under, as its row stands for it: the one the DSP last wrote to
    /// them in, while all it waits on answers a message to a group.
    pub group_id: Option<String>,
}

/// Whom a conversation is with: a member of a workspace by their account there, or someone texting;
/// and the person Driver Match knows them as, if it does.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Correspondent {
    /// Their ID in the workspace, for a member of one.
    pub user_id: Option<String>,
    /// The name Driver Match shows them by, else the one their Slack profile gives them, or
    /// the name the DSP saved for their number, or their number.
    pub name: String,
    /// Their person's code, which their profile and photo go by.
    pub code: Option<String>,
    pub role: Option<PersonRole>,
    pub title: Option<PersonTitle>,
    /// Their photo's version, for `/api/dsp/people/{code}/photo`.
    pub photo: Option<String>,
}

/// A message in a conversation, or a text.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    /// Its place among the conversation's messages, as `before` reads it: its ID in Slack, or
    /// for a text when it was written, thirteen digits, and its ID (`<at>:<id>`).
    pub ts: String,
    pub direction: Direction,
    pub text: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
    /// When it was last edited in Slack: only its latest text is kept.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub edited_at: Option<i64>,
    /// Whether it was written in a thread of the direct message.
    pub in_thread: bool,
    /// Who at the DSP sent it; none for the member's own.
    pub sender: Option<Sender>,
    pub files: Vec<MessageFile>,
    /// The group it went to, for one the DSP sent a group.
    pub group: Option<MessageGroup>,
    /// How a text Dispatch sent went.
    pub status: Option<TextStatus>,
    /// The other party's number, for a text: in a driver's conversation, the work phone it went
    /// through.
    pub number: Option<String>,
}

/// The group a message went to: its ID and name.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct MessageGroup {
    pub id: String,
    pub name: String,
}

/// Someone at the DSP who sent a message: by the name it went out under, their account, and
/// the person Driver Match knows them as, by their email address, if it does. A text sent from the
/// phone itself is the phone's, under its name, by no account but the phone's ID.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Sender {
    pub user_id: Option<String>,
    /// The DSP's phone that sent it from its own messaging app, for such a text.
    pub phone_id: Option<String>,
    pub name: String,
    pub code: Option<String>,
    pub photo: Option<String>,
}

/// `POST /api/dsp/communications/conversations/{id}/messages`: the message sent, and the
/// conversation it's in now, as it is: by text, another than the one written in once the number it
/// went to changed hands as it went, which the page follows.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    pub conversation: Conversation,
    pub message: Message,
}

/// A photo or file attached to a message, which `files/{id}` serves once it's kept.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct MessageFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: i64,
    pub status: FileStatus,
}

/// `GET /api/dsp/communications/conversations`: the fifty conversations found by name closest
/// to it, the closest first, or the answered ones before `before`, in every workspace or one, or
/// of the numbers that text the DSP (`kind=number`).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Conversations {
    pub conversations: Vec<Conversation>,
    /// The groups among the answered conversations; none for a search.
    pub groups: Vec<Group>,
    /// Whether older answered conversations and groups are left to read; never for a search.
    pub more: bool,
}

/// `GET /api/dsp/communications/conversations/{id}`: a conversation and its latest messages,
/// or those before the message `before`, by its `ts`, in the order written.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessages {
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    /// Whether older messages are left to read.
    pub more: bool,
    /// For a driver's work conversation, their route today and the work phone they hold now, as
    /// live route data holds them, while it holds today.
    pub today: Option<Today>,
    /// Whom it's with's conversation of each kind, this one too, while they have two or more:
    /// work, by text from their number, and Slack.
    pub elsewhere: Vec<Elsewhere>,
    /// For a driver's work conversation, the route and van of each day its messages were written.
    pub workdays: Vec<Workday>,
    /// What a message written in it may carry.
    pub limits: ComposeLimits,
}

/// What one message may carry: the most characters its text may have, how many photos, the most
/// bytes each may be, and their kinds.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct ComposeLimits {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub text: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub photos: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub largest: u64,
    pub kinds: Vec<String>,
}

/// One of a person's conversations of a kind, which the page goes to: its ID once they have one,
/// whether it waits, and what starts one: the number to text, or their account in a workspace.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Elsewhere {
    pub kind: ConversationKind,
    pub conversation_id: Option<String>,
    pub waiting: bool,
    /// For a work one, the work phone they hold now, if any; for a number's, the number.
    pub number: Option<String>,
    /// For a work one, their ID of the source naming who held the DSP's work phones.
    pub driver_id: Option<String>,
    /// For a Slack one, the workspace and their ID in it.
    pub team_id: Option<String>,
    pub user_id: Option<String>,
}

/// `GET /api/dsp/communications/conversations/{id}/profile`: what the profile beside a
/// conversation shows of the member it's with, besides the name, role and photo the
/// conversation has: their route today, and how to reach them. Each is left out when the
/// feature that would know it is off for the DSP, or knows none.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Their route on the DSP's own day today: as live route data last read it while it holds
    /// today, else as Routes has read it so far.
    pub today: Option<Today>,
    /// Their phone number and email address, the first Driver Match has from their sources.
    pub phone: Option<String>,
    pub email: Option<String>,
    /// The name of the workspace the conversation is in, while the DSP has it.
    pub slack: Option<String>,
}

/// Someone's route today: its code, what the DSP calls its van, when its wave starts, how many
/// stops it has in all and how many are done, and how it keeps up; and the work phone they hold
/// now, while live route data holds today, and since when.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Today {
    pub route: Option<String>,
    pub van: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub wave_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub stops: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub stops_done: Option<i64>,
    pub pace: Option<Pace>,
    /// As texts write it.
    pub phone: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub phone_since: Option<i64>,
}

/// `GET /api/dsp/communications/contacts`: the members of the DSP's workspaces, or of one of
/// them, by name, or those like what was searched, the closest first.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Contacts {
    pub contacts: Vec<Contact>,
}

/// A member of one of the DSP's workspaces someone at the DSP can message.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub team_id: String,
    pub person: Correspondent,
    /// Their conversation, if they have one.
    pub conversation_id: Option<String>,
    /// Since when their conversation waits on an answer, as `Conversation`'s.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since: Option<i64>,
    /// When their conversation's latest message was written, as `Conversation`'s.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_at: Option<i64>,
    /// The letters of their name that matched what was searched, as `Conversation`'s.
    pub matched: Vec<(u32, u32)>,
}

/// `POST /api/dsp/communications/uploads`, or `…/texts/uploads` for a text: a photo added for a
/// message not sent yet.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsUpload {
    pub id: String,
    pub name: String,
    pub mime: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: i64,
}
