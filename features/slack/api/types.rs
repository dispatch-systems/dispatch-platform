//! What Slack's endpoints answer with. Each type is written to TypeScript in `generated/`,
//! which `client.ts` imports.
use serde::Serialize;

/// `GET /api/dsp/slack/account`: the DSP's Slack workspaces and install link, as those who
/// manage its connections see them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackAccount {
    /// Whether this server has Slack's app to install.
    pub available: bool,
    pub workspaces: Vec<SlackWorkspace>,
    /// The install link, while it works.
    pub link: Option<SlackLink>,
}

/// A workspace the DSP added Dispatch to.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackWorkspace {
    pub team_id: String,
    /// Its name in Slack.
    pub name: String,
    /// Its name in Dispatch, if whoever added or renamed it gave it one.
    pub label: Option<String>,
    pub status: WorkspaceStatus,
    /// Whether the bot needs permissions its admin hasn't approved yet: updating asks for them.
    pub needs_update: bool,
    /// The IDs of the integrations on in it, as their features declare them.
    pub integrations: Vec<String>,
    /// The member who connected it, by name; none when it came by the install link, or they've
    /// left.
    pub connected_by: Option<String>,
    pub by_link: bool,
    pub connected_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Connected,
    /// Slack stopped honouring its tokens: reconnecting it mends it.
    Broken,
}

/// The DSP's install link, which works once, until it expires.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackLink {
    pub created_by: Option<String>,
    /// The name the workspace it adds is to have.
    pub label: Option<String>,
    /// When it stops working, in milliseconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
}

/// `POST /api/dsp/slack/link`: a new install link, shown once: Dispatch keeps only its hash.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackNewLink {
    pub url: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub expires_at: i64,
}

/// `POST /api/dsp/slack/connect`: where the browser goes to install Dispatch in Slack.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackInstall {
    pub url: String,
}

/// `GET /api/dsp/slack/workspaces/{team}/channels`: the workspace's channels Dispatch may be in,
/// by name.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackChannels {
    pub channels: Vec<SlackChannel>,
}

/// A channel of one of the DSP's workspaces: any public one, or a private one the bot was invited
/// to. Joining or leaving one answers with it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackChannel {
    pub id: String,
    pub name: String,
    pub private: bool,
    /// Whether the bot is in it.
    pub member: bool,
}

/// A folder where the DSP keeps its files.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackFolder {
    pub id: String,
    pub name: String,
}

/// `GET /api/dsp/slack/workspaces/{team}/imports/folders`: a folder, as choosing one browses it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackFolders {
    /// From the top down to it, itself last.
    pub path: Vec<SlackFolder>,
    /// The folders it holds, by name.
    pub folders: Vec<SlackFolder>,
}

/// Each of Slack's own integrations' answers, kept in its folder.
pub use crate::integrations::import_files::types::*;
pub use crate::integrations::sync_canvases::types::*;
