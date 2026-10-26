//! What Communications' endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports. Times are milliseconds since 1970.
pub use crate::backend::{
    files::FileStatus,
    groups::{GroupChoiceKind, GroupPersonState},
    storage::{ConversationState, Direction},
    texts::storage::{ConversationKind, TextStatus},
};
use dispatch_core::manifest::people::{PersonRole, PersonTitle};
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
    /// Whether the DSP texts: something offers it texting, its dispatch phones.
    pub texting: bool,
    pub typing: Typing,
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
    /// The number it texts: a number's own, or the work phone last texted with.
    pub number: Option<String>,
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

/// `GET /api/dsp/communications/texts/contacts`: whom the DSP can text, by name, or those like
/// what was searched, the closest first: everyone Driver Match has a phone number for who hasn't
/// left, and every number the DSP saved a name for; then the numbers blocked.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct TextContacts {
    pub contacts: Vec<TextContact>,
    pub blocked: Vec<TextContact>,
}
/// Someone the DSP can text, by their number.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct TextContact {
    pub number: String,
    pub person: Correspondent,
    /// Their number's conversation, if it has one.
    pub conversation_id: Option<String>,
    /// Since when it waits on an answer, and when it was last written in, as `Conversation`'s.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since: Option<i64>,
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

/// A group someone at the DSP messaged together, each in their own direct message, as the list
/// shows it among the conversations: the conversations listed under it are its.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    /// How many people it went to.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub people: i64,
    /// The first two of them, whose faces show it.
    pub faces: Vec<Correspondent>,
    /// The workspaces its people are in.
    pub team_ids: Vec<String>,
    /// Waiting while any conversation listed under it waits, since the longest has.
    pub state: ConversationState,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since: Option<i64>,
    /// When it was last written in, by the DSP or in an answer.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_at: i64,
    /// What was last written in it: the DSP's latest message to it, or the latest answer.
    pub last: Option<GroupEntry>,
}

/// One of a group's messages: one the DSP sent it, under who sent it, with how far it has gone;
/// or an answer, what one of its people wrote after it, with who and their conversation.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupEntry {
    pub id: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at: i64,
    pub text: String,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub edited_at: Option<i64>,
    pub files: Vec<MessageFile>,
    pub sender: Option<Sender>,
    pub progress: Option<GroupProgress>,
    pub from: Option<Correspondent>,
    pub conversation_id: Option<String>,
}
/// How far a group message has gone: to how many in all, how many it reached so far, and how
/// many it couldn't.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupProgress {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub people: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sent: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub failed: i64,
}

/// `GET /api/dsp/communications/groups/{id}`: a group and its latest messages and answers, or
/// those before `before`, in the order written.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupMessages {
    pub group: Group,
    pub entries: Vec<GroupEntry>,
    /// Whether older ones are left to read.
    pub more: bool,
}

/// `GET /api/dsp/communications/groups/{id}/people`: who a group went to, who sent it first and
/// when, and where it stands with each.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupPeople {
    pub sent_by: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub sent_at: i64,
    /// Those who answered first, the first to first, then the rest by name.
    pub people: Vec<GroupPerson>,
}
/// One of a group's people: who, in which workspace, their conversation once they have one, and
/// where the group stands with them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupPerson {
    pub team_id: String,
    pub person: Correspondent,
    pub conversation_id: Option<String>,
    pub state: GroupPersonState,
}

/// `GET /api/dsp/communications/groups/choices`: whom a group message can go to, each person
/// once with their workspace accounts, the best first; and the groups to choose from, each
/// holding someone: those on a route today and in each wave, by role, and with two or more
/// workspaces where Communications works, everyone and each workspace's.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupChoices {
    pub people: Vec<Reachable>,
    pub today: Vec<GroupChoice>,
    pub roles: Vec<GroupChoice>,
    pub workspaces: Vec<GroupChoice>,
}
/// Someone a group message can go to: one person, by their key (their person's code, else their
/// workspace and ID), and each of their accounts in a workspace where Communications works, the
/// one to message first.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct Reachable {
    pub key: String,
    pub person: Correspondent,
    pub members: Vec<GroupMember>,
}
/// A member of one of the DSP's workspaces: the workspace, and their ID there.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub team_id: String,
    pub user_id: String,
}
/// A group to choose from: what it is, as a wave by when it starts, a role, or a workspace, and
/// the people in it, by their keys.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct GroupChoice {
    pub kind: GroupChoiceKind,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub wave_at: Option<i64>,
    pub role: Option<PersonRole>,
    pub team_id: Option<String>,
    pub people: Vec<String>,
}
