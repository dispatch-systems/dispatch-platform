//! Communications' integration through Slack: what members of the DSP's workspaces write to the
//! app directly, kept for the DSP to read and answer on its Communications page, in place of
//! Slack answering that DSP Messaging is unavailable. On from the start in every workspace,
//! until someone who manages Slack switches it off there.
//!
//! - `incoming`: each message Slack tells of, and each edit, kept.
//! - `catch_up`: what Dispatch didn't hear of, read from Slack as the server starts and hourly.
//! - `fixture`: fixture mode's answers to the Slack methods only Communications calls.
pub mod catch_up;
pub mod fixture;
pub mod incoming;

use crate::backend::storage::CommunicationsStore;
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::observability,
    manifest::{
        Upkeep,
        integrations::{Heard, Integration, integration},
    },
};
use dispatch_slack::kit;
use serde_json::json;
use std::sync::Arc;

/// Its ID, as each workspace keeps it switched on or off. Permanent once shipped.
pub const ID: &str = "communications.messages";
/// What it is to Slack. It reads what's written to the app (`im:history`) and answers it
/// (`chat:write`), under the name and photo of whoever at the DSP does (`chat:write.customize`),
/// opens a direct message (`im:write`) and tells who members are (`users:read`,
/// `users:read.email`), which Slack asks for itself too; and it downloads what members attach
/// (`files:read`), uploads the photos someone at the DSP adds (`files:write`), and lists the app's
/// direct messages to catch up on them (`im:read`). It's told of each message written to the app
/// (`message.im`). Its `forget` keeps every message of a workspace forgotten, and lets go of what
/// holds none.
pub const INTEGRATION: Integration = Integration {
    needs: &[
        "im:history",
        "chat:write",
        "chat:write.customize",
        "im:write",
        "users:read",
        "users:read.email",
        "files:read",
        "files:write",
        "im:read",
    ],
    hears: &["message.im"],
    starts_on: true,
    heard: Some(heard),
    fixture: Some(fixture::answer),
    forget: Some(forget),
    ..integration("slack", ID, "Communications")
};

/// Whether the workspace's admin has approved all Communications uses there: else updating Slack
/// there asks for the rest.
pub fn approved(workspace: &kit::Workspace) -> bool {
    INTEGRATION
        .needs
        .iter()
        .all(|scope| workspace.scopes.iter().any(|had| had == scope))
}

/// Keeps what Slack told of, while it's on in the workspace and Communications is on for the
/// DSP. A failure is logged by its code alone, never with what was written.
pub fn heard(state: Arc<State>, heard: Heard) -> Upkeep {
    Box::pin(async move {
        let kept = incoming::heard(&state, &heard.dsp, &heard.account, &heard.event).await;
        if let Err(error) = kept {
            observability::event(
                "warn",
                "communications_heard_failed",
                json!({"error": error.code}),
            );
        }
    })
}

/// What it clears of a workspace Slack forgets: the conversations holding no message. Every
/// message stays, with who sent it and its photos and files, until the DSP is deleted.
fn forget(db: &Store, dsp: &str, team: &str) -> Result<()> {
    db.forget_empty_communications(dsp, team)
}

#[cfg(test)]
#[path = "tests/backend/messages.rs"]
mod tests;
