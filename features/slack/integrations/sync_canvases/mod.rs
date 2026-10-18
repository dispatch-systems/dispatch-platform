//! Sync Canvases, Slack's own integration: the canvases a DSP chooses are kept as documents where
//! it keeps its files (core's Files service, which Documents offers, as Google Docs), each
//! written over within minutes of the canvas being edited, never the other way. A DSP links a
//! channel to a folder, so every canvas in it is kept there, those to come too; or chooses
//! canvases of a channel's on their own. Slack doesn't tell apps which of a channel's folders
//! holds a canvas, so a folder of Slack's is chosen as its canvases. Switched on in a
//! workspace's Integrations, it's told of each file shared and changed where Dispatch is.
//!
//! - `choices`: the canvases chosen on their own, and the canvases of a channel to choose from;
//!   the channels are linked by `channel_links`, as `LINKING` declares.
//! - `overview`: what the workspace's settings show of it.
//! - `queue`: each canvas to be synced, kept until it is, the minute's pass that syncs what's
//!   due, and what Slack tells of; `check`, the pass that finds the canvases and edits Slack
//!   didn't tell of.
//! - `sync`: syncing one canvas, and `page`, a canvas as a page to write.
//! - `storage`: what's kept.
pub(crate) mod api;
pub mod check;
pub mod choices;
pub mod overview;
pub mod page;
pub mod queue;
pub mod storage;
mod sync;
pub(crate) mod types;

use crate::backend::channel_links::Linking;
use dispatch_core::{
    db::Db,
    manifest::integrations::{Integration, integration},
};

/// Its ID, as each workspace keeps it switched on.
pub const ID: &str = "slack.sync_canvases";
/// What it is to Slack: it reads the canvases and pictures in the channels Dispatch is in
/// (`files:read`), and is told of each one shared there (`file_shared`) and of each change to
/// one (`file_change`).
pub const INTEGRATION: Integration = Integration {
    needs: &["files:read"],
    hears: &["file_shared", "file_change"],
    heard: Some(queue::heard),
    ..integration("slack", ID, "Sync Canvases")
};
/// Its channels linked to folders, each of whose canvases is kept there.
pub const LINKING: Linking = Linking {
    integration: ID,
    table: "slack_canvas_links",
    off: "slack_canvases_off",
    linked: "slack.canvases_linked",
    moved: "slack.canvases_moved",
    unlinked: "slack.canvases_unlinked",
    // An unlinked channel's canvases stop syncing; each is kept for its document to be written
    // over again should the channel be linked again, or the canvas be chosen.
    unlinking: |_: &Db, _: &str, _: &str| Ok(()),
};
