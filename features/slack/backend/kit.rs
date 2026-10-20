//! What another feature's integration reads of Slack beside the bot: the DSP's workspaces and
//! their members, as Slack keeps them, which workspaces an integration works in and since when,
//! and the IDs Driver Match knows each member by. Only reads: what Slack keeps, Slack alone
//! writes.
use super::{
    people,
    storage::{Member, SlackStore, Workspace},
    work,
};
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// The DSP's workspaces, by name.
pub fn workspaces(db: &Store, dsp: &str) -> Result<Vec<Workspace>> {
    db.slack_workspaces(dsp)
}
/// Every member of the DSP's workspaces, with their workspace: bots aren't members.
pub fn members(db: &Store, dsp: &str) -> Result<Vec<(String, Member)>> {
    db.slack_members(dsp)
}
/// The DSP's workspaces the integration `integration` works in: each working workspace it's on
/// in, by its team ID.
pub fn workspaces_on(db: &Store, dsp: &str, integration: &str) -> Result<Vec<String>> {
    work::workspaces_on(db, dsp, integration)
}
/// When the integration `integration` came on in the workspace `team`, in milliseconds: when
/// someone switched it on there, or, for one on from the start, when the DSP got the workspace,
/// however often it was reconnected since. None while it's off.
pub fn integration_on_since(
    db: &Store,
    dsp: &str,
    team: &str,
    integration: &str,
) -> Result<Option<i64>> {
    db.slack_integration_on_since(dsp, team, integration)
}
/// The source of the IDs Driver Match knows the members by.
pub const SOURCE: Source = people::SOURCE;
/// The ID Driver Match knows the workspace `team`'s member `user` by.
pub fn member_id(team: &str, user: &str) -> String {
    format!("{team}:{user}")
}
