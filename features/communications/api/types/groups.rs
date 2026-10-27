//! What messaging a group is answered with: a group, its messages and answers, its people, and
//! whom a group message can go to.
use super::{
    ConversationState, Correspondent, GroupChoiceKind, GroupPersonState, MessageFile, Sender,
};
use dispatch_core::manifest::people::PersonRole;
use serde::Serialize;

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
