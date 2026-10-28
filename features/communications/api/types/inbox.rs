//! What the list's endpoints answer with: the overview, what waits, its updates, the DSP's
//! workspaces and phones, and who is writing.
use super::{Conversation, Group, Message};
use serde::Serialize;

/// `GET /api/dsp/communications`: the DSP's conversations, those waiting first, the longest
/// waiting first, then the latest answered of its Slack workspaces, every one or the one asked
/// for, and of its texts, with the workspaces they're in, whether it texts, and who is typing.
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
    /// The groups waiting, then those answered among the conversations answered.
    pub groups: Vec<Group>,
    /// Whether older answered Slack conversations and groups are left to read, with
    /// `conversations?before=`, of the same workspaces.
    pub more: bool,
    /// Whether older answered numbers' conversations are left to read, with
    /// `conversations?kind=number&before=`.
    pub more_texts: bool,
    pub workspaces: Vec<CommunicationsWorkspace>,
    /// Whether Messaging is offered: always, empty until a phone is paired and texted.
    pub texting: bool,
    /// Whether Today's Routes is offered: the DSP has live route data.
    pub routes: bool,
    /// The DSP's dispatch phones: none until one is paired.
    pub phones: Vec<DispatchPhone>,
    pub typing: Typing,
}

/// One of the DSP's dispatch phones, and whether it's online.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct DispatchPhone {
    pub id: String,
    pub name: String,
    /// Whether it has checked in lately.
    pub online: bool,
    /// When it last checked in; none before it ever has.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_seen_at: Option<i64>,
}

/// `GET /api/dsp/communications/waiting`: how many conversations wait on an answer, for the
/// sidebar, since when each has, and what each waits on, which alerts the member to a new message.
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
    /// The latest message from whom each is with, the longest waiting first: a message not there
    /// before is a new one.
    pub latest: Vec<WaitingMessage>,
}

/// The latest message from whom a waiting conversation is with: its ID, the group it's listed
/// under, if any, who they are, and what they wrote.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct WaitingMessage {
    pub conversation_id: String,
    pub group_id: Option<String>,
    /// Their name, as the conversation's.
    pub from: String,
    pub message: Message,
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
    /// The version of its logo, which `workspaces/{team}/logo?v=` shows: none for a workspace
    /// without one of its own, or whose logo hasn't come yet.
    pub logo: Option<String>,
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
/// edited or a file kept, and a group with its messages going out and its answers, so a page
/// with one open reads it again when it's among them. The
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
    /// Each group changed, as it is now.
    pub groups: Vec<Group>,
    pub typing: Typing,
    pub workspaces: Vec<CommunicationsWorkspace>,
    pub texting: bool,
    pub routes: bool,
    pub phones: Vec<DispatchPhone>,
}
