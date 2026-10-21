//! What another feature's integration reads of Slack beside the bot: the DSP's workspaces, their
//! logos and their members, as Slack keeps them, which workspaces an integration works in and
//! since when, and the IDs Driver Match knows each member by. Only reads: what Slack keeps,
//! Slack alone writes.
use super::{
    logo, people,
    storage::{Member, SlackStore, Workspace},
    work,
};
use dispatch_core::{
    Result,
    db::Store,
    manifest::people::{Photo, Source},
};
use std::collections::BTreeMap;

/// The DSP's workspaces, by name.
pub fn workspaces(db: &Store, dsp: &str) -> Result<Vec<Workspace>> {
    db.slack_workspaces(dsp)
}
/// The version of each logo Slack keeps of the DSP's workspaces, by team ID: what the address
/// that shows it carries, which changes as the logo does.
pub fn logos(db: &Store, dsp: &str) -> Result<BTreeMap<String, String>> {
    db.slack_logos(dsp)
}
/// The workspace `team`'s logo, as Slack keeps a copy of it, to show: its kind, as `image/png`,
/// and its picture. None for a workspace without one of its own, or whose logo hasn't come yet.
pub fn logo(db: &Store, dsp: &str, team: &str) -> Result<Option<Photo>> {
    logo::picture(db, dsp, team)
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
