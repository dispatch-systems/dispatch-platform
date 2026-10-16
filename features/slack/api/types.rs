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
