//! What Communications' endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports. Times are milliseconds since 1970.
pub use crate::backend::{
    files::FileStatus,
    storage::{ConversationState, Direction},
};
use dispatch_core::manifest::people::PersonRole;
use serde::Serialize;

/// `GET /api/dsp/communications`: the DSP's conversations, those waiting first, the longest
/// waiting first, then the latest answered, of every workspace or the one asked for, with the
/// workspaces they're in and who is typing.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Communications {
    /// The latest change, for `updates` to wait on.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    /// How many conversations wait on an answer.
    pub waiting: usize,
    pub conversations: Vec<Conversation>,
    /// Whether older answered conversations are left to read, with `conversations?before=`, of
    /// the same workspaces.
    pub more: bool,
    pub workspaces: Vec<CommunicationsWorkspace>,
    pub typing: Typing,
}

/// `GET /api/dsp/communications/waiting`: how many conversations wait on an answer, for the
/// sidebar, and since when each has.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsWaiting {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    pub count: usize,
    /// Since when each waits, the longest waiting first.
    #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
    pub since: Vec<i64>,
}

/// One of the DSP's Slack workspaces, where its members write to the app.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsWorkspace {
    pub team_id: String,
    /// Its name in Dispatch, else its name in Slack.
    pub name: String,
    /// Whether Communications works there now: its access to Slack working, and Communications
    /// on there.
    pub on: bool,
    /// The address that opens the app's Messages tab in the workspace, where its members write
    /// to the DSP.
    pub chat_link: String,
    /// Whether, Communications being on there, the workspace's admin has yet to approve a
    /// permission it uses: updating Slack there asks for it.
    pub needs_update: bool,
}

/// A conversation with a member of one of the DSP's workspaces.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    /// The workspace it's in.
    pub team_id: String,
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
}

/// The member a conversation is with: their account in the workspace, and the person Driver
/// Match knows them as, if it does.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Correspondent {
    /// Their ID in the workspace.
    pub user_id: String,
    /// The name Driver Match shows them by, else the one their Slack profile gives them.
    pub name: String,
    /// Their person's code, which their profile and photo go by.
    pub code: Option<String>,
    pub role: Option<PersonRole>,
    /// Their photo's version, for `/api/dsp/people/{code}/photo`.
    pub photo: Option<String>,
}

/// A message in a conversation.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    /// Its ID in Slack, which orders a conversation's messages, as `before` reads it.
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
}

/// Someone at the DSP who sent a message: by the name it went out under, their account, and
/// the person Driver Match knows them as, by their email address, if it does.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Sender {
    pub user_id: String,
    pub name: String,
    pub code: Option<String>,
    pub photo: Option<String>,
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
/// to it, the closest first, or the answered ones before `before`, in every workspace or one.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Conversations {
    pub conversations: Vec<Conversation>,
    /// Whether older answered conversations are left to read; never for a search.
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
    /// Their route on the DSP's own day today, as Routes has read it so far.
    pub today: Option<ProfileToday>,
    /// Their phone number and email address, the first Driver Match has from their sources.
    pub phone: Option<String>,
    pub email: Option<String>,
    /// The name of the workspace the conversation is in, while the DSP has it.
    pub slack: Option<String>,
}
/// Someone's route today: its code, what the DSP calls its van, when its wave starts, and how
/// many stops it has in all.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct ProfileToday {
    pub route: Option<String>,
    pub van: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub wave_at: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub stops: Option<i64>,
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

/// Who at the DSP is writing in which conversation now, and the version of that, for `updates`.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Typing {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub version: u64,
    pub writing: Vec<Writing>,
}
/// Someone at the DSP writing in a conversation, other than whoever asks.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Writing {
    pub conversation_id: String,
    pub user_id: String,
    pub first_name: String,
    pub name: String,
    /// The person Driver Match knows them as, by their email address, if it does, and their
    /// photo's version, as a message's sender's.
    pub code: Option<String>,
    pub photo: Option<String>,
}

/// `GET /api/dsp/communications/updates`: what changed since `after`, and who is typing, once
/// either changes or twenty seconds pass. `reset` asks for `GET /api/dsp/communications` afresh,
/// as for changes no longer kept. A conversation changes with any of its messages, as one
/// edited or a file kept, so a page with one open reads it again when it's among them. The
/// workspaces come as they are now, so a page follows one connected, updated, switched off or
/// disconnected while it waits.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsUpdates {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: i64,
    pub reset: bool,
    pub waiting: usize,
    /// Each conversation changed, as it is now.
    pub conversations: Vec<Conversation>,
    pub typing: Typing,
    pub workspaces: Vec<CommunicationsWorkspace>,
}

/// `POST /api/dsp/communications/uploads`: a photo added for a message not sent yet.
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
