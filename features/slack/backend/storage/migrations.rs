//! Historical Rust migrations; their IDs and registration live in the feature manifest.
use dispatch_core::{
    Result,
    db::{Db, migrations::add_column},
};

/// Slack's migration 2, for adding Slack from New on Settings' DSP Connections. Each workspace's
/// name in Dispatch, `label`, which whoever adds it may give it; without one it's known by its
/// name in Slack. An install a member started: the workspace it named to Slack, to reconnect or
/// update it, none for one started to add a workspace, which mustn't then be one the DSP already
/// has; and, like the install link, the name the workspace it adds is to have.
pub(crate) fn add_new_connections(db: &Db) -> Result<()> {
    add_column(db, "slack_workspaces", "label", "TEXT")?;
    add_column(db, "slack_connect_requests", "team_id", "TEXT")?;
    add_column(db, "slack_connect_requests", "label", "TEXT")?;
    add_column(db, "slack_install_links", "label", "TEXT")
}

/// Slack's migration 8: when the DSP got each workspace (`added_at`), which reconnecting or
/// updating it leaves as it was, unlike when it was last connected: for one it had already, when
/// it was connected.
pub(crate) fn add_workspaces_added(db: &Db) -> Result<()> {
    add_column(db, "slack_workspaces", "added_at", "TEXT")?;
    db.exec(
        "UPDATE slack_workspaces SET added_at=connected_at WHERE added_at IS NULL",
        [],
    )?;
    Ok(())
}
/// Slack's migration 10: the DSP page an install a member started returns to (`page`), when it
/// started on another than Settings' Connections, as on Communications' page.
pub(crate) fn add_install_pages(db: &Db) -> Result<()> {
    add_column(db, "slack_connect_requests", "page", "TEXT")
}
