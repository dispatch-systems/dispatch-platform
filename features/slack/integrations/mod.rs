//! Slack's own integrations, a folder each: what it does in a DSP's workspaces once someone who
//! manages Slack switches it on there. Each folder holds all of one: its declaration, what it
//! keeps and does (`mod.rs` and its modules), its endpoints (`api.rs`) and the types they answer
//! with (`types.rs`), its frontend (its card and section, their calls and its audit wording),
//! its migrations and its tests. Slack's manifest lists each one's integration, upkeep,
//! migration and tables; removing a workspace clears every table Slack declares by its team.
//!
//! - `import_files`: Import Files, saving what's posted in linked channels to folders.
//! - `sync_canvases`: Sync Canvases, keeping a channel's canvases as documents in folders.
pub(crate) mod import_files;
pub(crate) mod sync_canvases;

use dispatch_core::{Result, server::http::route::Route};
use serde_json::Value;

/// One of Slack's own integrations, as its folder declares it beside core's `Integration`.
pub(crate) struct Own {
    /// Its endpoints, on a workspace's settings.
    pub routes: fn() -> Vec<Route>,
    /// Fixture mode's answer to a Slack method only it calls, in the workspace `team`, or none
    /// for a method Slack's fixture answers: previews and tests never reach Slack.
    pub fixture: fn(team: &str, method: &str, args: &Value) -> Option<Result<Value>>,
}
/// What answers no method, for an integration whose calls Slack's fixture answers.
pub(crate) fn no_fixture(_: &str, _: &str, _: &Value) -> Option<Result<Value>> {
    None
}

/// Every one of Slack's own integrations.
pub(crate) const ALL: &[&Own] = &[&import_files::OWN, &sync_canvases::OWN];
