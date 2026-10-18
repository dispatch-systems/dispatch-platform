//! Import Files, Slack's own integration: the files and photos posted in the channels a DSP links
//! to folders are saved there, where the DSP keeps its files (the `files` contract, which
//! Documents exposes), never their messages. Switched on in a workspace's Integrations, it's told
//! of each file shared in a channel Dispatch is in, and saves those posted in a linked channel
//! from when it was linked, while it's on.
//!
//! - `links`: what the workspace's settings show of it: its linked channels, which
//!   `channel_links` links to folders as it does Sync Canvases', and what's been saved.
//! - `queue`: each file noticed, kept until it's saved, and the minute's pass that saves what's
//!   due.
//! - `save`: saving one file, and who posted it, as `author` says.
//! - `catch_up`: the hourly pass that notices the files Slack didn't tell of.
//! - `storage`: what's kept.
pub(crate) mod api;
pub mod catch_up;
pub mod links;
pub mod queue;
mod save;
pub mod storage;
pub(crate) mod types;

use crate::backend::channel_links::Linking;
use dispatch_core::manifest::integrations::{Integration, integration};

/// Its endpoints, on a workspace's settings.
pub(crate) const OWN: super::Own = super::Own {
    routes: api::routes,
};
/// Its ID, as each workspace keeps it switched on.
pub const ID: &str = "slack.import_files";
/// What it is to Slack: it reads the files in the channels Dispatch is in (`files:read`), and is
/// told of each one shared there (`file_shared`).
pub const INTEGRATION: Integration = Integration {
    needs: &["files:read"],
    hears: &["file_shared"],
    heard: Some(queue::heard),
    ..integration("slack", ID, "Import Files")
};
/// Its channels linked to folders, each of whose files posted since it was linked is saved there.
pub const LINKING: Linking = Linking {
    integration: ID,
    table: "slack_import_links",
    off: "slack_import_off",
    linked: "slack.import_linked",
    moved: "slack.import_moved",
    unlinked: "slack.import_unlinked",
    // What was waiting from an unlinked channel isn't saved.
    unlinking: storage::forget_waiting,
};
