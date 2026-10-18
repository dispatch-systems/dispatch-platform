//! What Import Files's endpoints answer with, written to TypeScript beside Slack's own in
//! `api/generated/`, which Slack's `client.ts` imports.
use crate::api::types::SlackFolder;
use serde::Serialize;

/// `GET /api/dsp/slack/workspaces/{team}/imports`: Import Files in the workspace, as its settings
/// show it: where files go, the channels linked, and the files noticed latest.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackImports {
    /// What keeps the DSP's files, by name: "Documents".
    pub store: String,
    /// Why files can't be saved yet, in words a member reads; none once they can.
    pub unready: Option<String>,
    pub links: Vec<SlackImportLink>,
    pub recent: Vec<SlackImportedFile>,
}

/// A channel linked to the folder its files are saved to.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackImportLink {
    pub channel_id: String,
    pub channel_name: String,
    pub private: bool,
    pub folder: SlackFolder,
    /// Whether the folder is gone, so its files can't be saved until another is chosen.
    pub folder_missing: bool,
    /// How many of its files were saved, and when the last was.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub saved: i64,
    pub last_saved_at: Option<String>,
}

/// A file noticed in a linked channel, and how saving it went.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct SlackImportedFile {
    pub file_id: String,
    /// Its name and type, once Slack described it.
    pub name: Option<String>,
    pub mime: Option<String>,
    /// The channel it was posted in, by name, while it's linked.
    pub channel_name: Option<String>,
    /// Who posted it: the member they are, or the name Slack shows them by.
    pub posted_by: Option<String>,
    pub status: SlackImportStatus,
    /// Whether a failure is being tried again.
    pub retrying: bool,
    /// Where it opens, once saved.
    pub link: Option<String>,
    pub noticed_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/slack/api/generated/"))]
#[serde(rename_all = "snake_case")]
pub enum SlackImportStatus {
    Waiting,
    Saved,
    /// Larger than where it goes takes.
    TooLarge,
    /// Deleted in Slack before it was saved.
    Gone,
    Failed,
}
