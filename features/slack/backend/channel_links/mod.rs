//! Channels linked to folders, for each of Slack's own integrations that keeps what's in a
//! channel where the DSP keeps its files (core's Files service): Import Files keeps a linked
//! channel's files there, Sync Canvases its canvases. Each keeps its links apart, in a table of
//! its own, and changes them as the workspace's settings do: for a member who manages Slack and
//! sees the folders where the DSP keeps its files, while the integration is on in the
//! workspace. A channel Dispatch is in is linked once, to a folder chosen or a new one named
//! after it; the activity log records each change.
//!
//! A new integration of the kind declares its `Linking`, with a table like
//! `slack_import_links`, and calls these from its routes.
pub mod storage;

use super::{bot::Bot, channels, storage::SlackStore};
use crate::api::types::{SlackFolder, SlackFolders};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{Db, Store, now},
    ensure,
    manifest::services::files::{self, Author, Files, Folder},
    server::http::route::Dsp,
};
use serde_json::json;
use std::sync::Arc;
use storage::{Link, LinkStore};

/// One of Slack's integrations that links channels to folders.
pub struct Linking {
    /// The integration, by its ID: its links change only while it's on in the workspace.
    pub integration: &'static str,
    /// The table its links are kept in.
    pub table: &'static str,
    /// How it's refused while it's off in the workspace.
    pub off: &'static str,
    /// What the activity log calls linking a channel, choosing it another folder, and
    /// unlinking it.
    pub linked: &'static str,
    pub moved: &'static str,
    pub unlinked: &'static str,
    /// What goes with a channel's link, in the DSP's database, as the link goes: given the
    /// workspace and the channel.
    pub unlinking: fn(&Db, &str, &str) -> Result<()>,
}

/// What keeps the DSP's files, for the member `c` in the workspace `team`: only while `of` is
/// on there, something keeps the DSP's files, and the member sees its folders.
pub async fn store(
    state: &Arc<State>,
    of: &'static Linking,
    c: &Context,
    team: &str,
) -> Result<&'static dyn Files> {
    let (dsp, at) = (c.dsp.id.clone(), team.to_owned());
    let (files, workspace, on) = state
        .read(move |db| {
            Ok((
                files::for_dsp(db, &dsp)?,
                db.slack_workspace(&dsp, &at)?,
                db.slack_integrations(&dsp, &at)?
                    .iter()
                    .any(|id| id == of.integration),
            ))
        })
        .await?;
    ensure(workspace.is_some(), "slack_workspace_not_found", 404)?;
    ensure(on, of.off, 409)?;
    let files = files.ok_or_else(|| Error::new("slack_files_unavailable", 404))?;
    ensure(c.can(files.permission()), "slack_files_forbidden", 403)?;
    Ok(files)
}

/// Links the channel `channel` Dispatch is in to the folder `folder`, or to a new folder named
/// after it.
pub async fn link(
    state: &Arc<State>,
    of: &'static Linking,
    c: &Context,
    access: Dsp,
    team: &str,
    channel: &str,
    folder: Option<String>,
) -> Result<()> {
    let files = store(state, of, c, team).await?;
    let dsp = c.dsp.id.clone();
    ensure(
        files.unready(state, &dsp).await?.is_none(),
        "slack_files_unready",
        409,
    )?;
    let bot = Bot::new(state, &dsp, team);
    let info = bot
        .call("conversations.info", &json!({"channel": channel}))
        .await?;
    let seen =
        channels::described(&info["channel"]).ok_or_else(|| Error::new("slack_refused", 502))?;
    ensure(seen.member, "slack_channel_not_joined", 409)?;
    let folder = match folder {
        Some(id) => chosen(state, files, &dsp, &id).await?,
        None => {
            let by = Author::Member(c.actor().to_owned());
            files.create_folder(state, &dsp, &seen.name, &by).await?
        }
    };
    let link = Link {
        team: team.to_owned(),
        channel: seen.id,
        channel_name: seen.name,
        private: seen.private,
        folder: folder.id,
        folder_name: folder.name,
        linked_by: c.actor().to_owned(),
        linked_at: now(),
    };
    let asked = c.clone();
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let linked = db.slack_channel_link(&c.dsp.id, of, &link.team, &link.channel)?;
            ensure(linked.is_none(), "slack_channel_linked", 409)?;
            db.save_slack_channel_link(&c.dsp.id, of, &link)?;
            record(db, &c, of.linked, &link)
        })
        .await
}

/// Chooses the linked channel `channel` the folder `folder`, from now on.
pub async fn move_to(
    state: &Arc<State>,
    of: &'static Linking,
    c: &Context,
    access: Dsp,
    team: &str,
    channel: String,
    folder: String,
) -> Result<()> {
    let files = store(state, of, c, team).await?;
    let folder = chosen(state, files, &c.dsp.id, &folder).await?;
    let (asked, at) = (c.clone(), team.to_owned());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let linked = db.slack_channel_link(&c.dsp.id, of, &at, &channel)?;
            let mut link = linked.ok_or_else(|| Error::new("slack_channel_not_linked", 404))?;
            link.folder = folder.id;
            link.folder_name = folder.name;
            db.save_slack_channel_link(&c.dsp.id, of, &link)?;
            record(db, &c, of.moved, &link)
        })
        .await
}

/// Unlinks the channel `channel`, with what goes with its link.
pub async fn unlink(
    state: &Arc<State>,
    of: &'static Linking,
    c: &Context,
    access: Dsp,
    team: &str,
    channel: String,
) -> Result<()> {
    store(state, of, c, team).await?;
    let (asked, at) = (c.clone(), team.to_owned());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let linked = db.slack_channel_link(&c.dsp.id, of, &at, &channel)?;
            let link = linked.ok_or_else(|| Error::new("slack_channel_not_linked", 404))?;
            db.remove_slack_channel_link(&c.dsp.id, of, &at, &channel)?;
            record(db, &c, of.unlinked, &link)
        })
        .await
}

/// The folder `folder`, or the top of where the DSP keeps its files, as choosing a folder for
/// the workspace `team` browses it.
pub async fn browse(
    state: &Arc<State>,
    of: &'static Linking,
    c: &Context,
    team: &str,
    folder: Option<String>,
) -> Result<SlackFolders> {
    let files = store(state, of, c, team).await?;
    let browsed = files.browse(state, &c.dsp.id, folder.as_deref()).await?;
    let shown = |folder: Folder| SlackFolder {
        id: folder.id,
        name: folder.name,
    };
    Ok(SlackFolders {
        path: browsed.path.into_iter().map(shown).collect(),
        folders: browsed.folders.into_iter().map(shown).collect(),
    })
}

/// The folder `link` keeps its channel's things in, as it is now once they can be kept, and as
/// it was last seen until then: none once it's gone.
pub async fn folder_now(
    state: &Arc<State>,
    files: &'static dyn Files,
    dsp: &str,
    ready: bool,
    link: &Link,
) -> Result<(SlackFolder, bool)> {
    let now_is = match ready {
        true => Some(files.folder(state, dsp, &link.folder).await?),
        false => None,
    };
    let folder = SlackFolder {
        id: link.folder.clone(),
        name: now_is
            .clone()
            .flatten()
            .map_or_else(|| link.folder_name.clone(), |folder| folder.name),
    };
    Ok((folder, matches!(now_is, Some(None))))
}

/// The folder `id`, chosen for a channel: one where the DSP keeps its files.
async fn chosen(
    state: &Arc<State>,
    files: &'static dyn Files,
    dsp: &str,
    id: &str,
) -> Result<Folder> {
    files
        .folder(state, dsp, id)
        .await?
        .ok_or_else(|| Error::new("files_folder_not_found", 404))
}

/// Records a change to the link `link` in the activity log, once the member is checked again:
/// `#<channel> to <folder> in <workspace>`.
fn record(db: &Store, c: &Context, action: &str, link: &Link) -> Result<()> {
    let workspace = db
        .slack_workspace(&c.dsp.id, &link.team)?
        .map(|workspace| workspace.label.unwrap_or(workspace.name))
        .unwrap_or_default();
    let detail = format!(
        "#{} to {} in {workspace}",
        link.channel_name, link.folder_name
    );
    c.audit(db, action, &detail)
}
