//! Communications, Communications's integration through Slack: TODO(scaffold): what it does in a DSP's
//! workspaces once someone who manages Slack switches it on there, in a sentence or two.

use dispatch_core::{
    State,
    manifest::{
        Upkeep,
        integrations::{Heard, Integration, integration},
    },
};
use std::sync::Arc;

/// Its ID, as each workspace keeps it switched on. Permanent once shipped.
pub const ID: &str = "communications.messages";
/// What it is to Slack: the permissions it needs of Slack, and the events it's told of. A
/// Slack method only it calls is answered in fixture mode by its `fixture`, so previews and
/// tests never reach Slack. Once it keeps rows by workspace, its `forget` clears them as
/// Slack forgets one.
pub const INTEGRATION: Integration = Integration {
    needs: &["files:read", "files:write", "im:read"],
    hears: &["message.im"],
    heard: Some(heard),
    ..integration("slack", ID, "Communications")
};

/// What it does with each event it's told of, while it's on in the workspace and its feature
/// is on for the DSP: `dispatch_slack::kit::Bot` acts in the workspace.
pub fn heard(state: Arc<State>, heard: Heard) -> Upkeep {
    Box::pin(async move {
        // TODO(scaffold): handle each event it hears of.
        let _ = (state, heard);
    })
}
