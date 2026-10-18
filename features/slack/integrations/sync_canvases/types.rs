//! What Sync Canvases's endpoints answer with, written to TypeScript beside Slack's own in
//! `api/generated/`, which Slack's `client.ts` imports.
use crate::api::types::SlackFolder;
use serde::Serialize;

/// `GET /api/dsp/slack/workspaces/{team}/canvases`: Sync Canvases in the workspace, as its
/// settings show it: where documents go, the channels linked, and every canvas kept.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackCanvases {
    /// What keeps the DSP's files, by name: "Documents".
    pub store: String,
    /// Why documents can't be written yet, in words a member reads; none once they can.
    pub unready: Option<String>,
    pub links: Vec<SlackCanvasLink>,
    pub canvases: Vec<SlackCanvas>,
}

/// A channel linked to the folder its canvases are kept in.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackCanvasLink {
    pub channel_id: String,
    pub channel_name: String,
    pub private: bool,
    pub folder: SlackFolder,
    /// Whether the folder is gone, so its new canvases can't be kept until another is chosen.
    pub folder_missing: bool,
    /// How many of its canvases are kept as documents.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub kept: i64,
}

/// A canvas kept as a document, or to be, and how syncing it goes.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackCanvas {
    pub canvas_id: String,
    pub title: String,
    /// The channel it's in, by name.
    pub channel_name: Option<String>,
    /// The folder it was chosen for on its own; none for one its channel's link covers.
    pub folder: Option<SlackFolder>,
    pub folder_missing: bool,
    pub status: SlackCanvasStatus,
    /// Whether a failure is being tried again.
    pub retrying: bool,
    /// Where its document opens, once written.
    pub link: Option<String>,
    /// When its document was last written.
    pub synced_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum SlackCanvasStatus {
    /// To be written, or written over.
    Waiting,
    Synced,
    /// Deleted in Slack: its document stays as it was.
    Gone,
    /// Couldn't be written: tried again once it's edited.
    Failed,
}

/// `GET /api/dsp/slack/workspaces/{team}/canvases/channels/{channel}`: a channel's canvases, to
/// choose from.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackChannelCanvases {
    /// Whether the channel is linked, so every canvas in it is kept.
    pub linked: bool,
    pub canvases: Vec<SlackChannelCanvas>,
}

/// A canvas in a channel, to choose.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackChannelCanvas {
    pub id: String,
    pub title: String,
    /// Whether it's chosen already.
    pub chosen: bool,
}
