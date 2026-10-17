//! The channels a workspace links to folders, as its Import Files settings change them, and what
//! they've saved: for a member who manages Slack and sees the folders where the DSP keeps its
//! files, while Import Files is on in the workspace. A channel Dispatch is in is linked once, to
//! a folder chosen or a new one named after it; the activity log records each change.
use super::{
    ID,
    storage::{Import, ImportStore, Link, Status},
};
use crate::{
    api::types::{
        SlackFolder, SlackFolders, SlackImportLink, SlackImportStatus, SlackImportedFile,
        SlackImports,
    },
    backend::{bot::Bot, channels, storage::SlackStore},
};
use dispatch_core::{
    Error, Result, State,
    accounts::Context,
    db::{at, now},
    ensure,
    manifest::services::files::{self, Author, Files, Folder},
    server::http::route::Dsp,
};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

/// The files noticed latest the settings show.
const RECENT: usize = 20;

/// What keeps the DSP's files, for the member `c` in the workspace `team`: only while Import
/// Files is on there, something keeps the DSP's files, and the member sees its folders.
async fn store(state: &Arc<State>, c: &Context, team: &str) -> Result<&'static dyn Files> {
    let (dsp, of) = (c.dsp.id.clone(), team.to_owned());
    let (files, workspace, on) = state
        .read(move |db| {
            Ok((
                files::for_dsp(db, &dsp)?,
                db.slack_workspace(&dsp, &of)?,
                db.slack_integrations(&dsp, &of)?.iter().any(|id| id == ID),
            ))
        })
        .await?;
    ensure(workspace.is_some(), "slack_workspace_not_found", 404)?;
    ensure(on, "slack_import_off", 409)?;
    let files = files.ok_or_else(|| Error::new("slack_files_unavailable", 404))?;
    ensure(c.can(files.permission()), "slack_files_forbidden", 403)?;
    Ok(files)
}

/// Import Files in the workspace `team`: where files go and whether they can be saved now, its
/// linked channels with their folders as they are now and what each saved, and the files
/// noticed latest.
pub async fn overview(state: &Arc<State>, c: &Context, team: &str) -> Result<SlackImports> {
    let files = store(state, c, team).await?;
    let dsp = c.dsp.id.clone();
    let unready = files.unready(state, &dsp).await?;
    let (at_dsp, of) = (dsp.clone(), team.to_owned());
    let (links, totals, recent) = state
        .read(move |db| {
            Ok((
                db.slack_import_links(&at_dsp, &of)?,
                db.slack_import_totals(&at_dsp, &of)?,
                db.recent_slack_imports(&at_dsp, &of, RECENT)?,
            ))
        })
        .await?;
    let totals: BTreeMap<String, (i64, Option<i64>)> = totals
        .into_iter()
        .map(|(channel, saved, last)| (channel, (saved, last)))
        .collect();
    let mut shown = Vec::new();
    for link in &links {
        // The folder as it is now, once files can be kept; as last seen until then.
        let now_is = match unready {
            None => Some(files.folder(state, &dsp, &link.folder).await?),
            Some(_) => None,
        };
        let (saved, last) = totals.get(&link.channel).copied().unwrap_or((0, None));
        shown.push(SlackImportLink {
            channel_id: link.channel.clone(),
            channel_name: link.channel_name.clone(),
            private: link.private,
            folder: SlackFolder {
                id: link.folder.clone(),
                name: now_is
                    .clone()
                    .flatten()
                    .map_or_else(|| link.folder_name.clone(), |folder| folder.name),
            },
            folder_missing: matches!(now_is, Some(None)),
            saved,
            last_saved_at: last.map(at),
        });
    }
    let names: BTreeMap<&str, &str> = links
        .iter()
        .map(|link| (link.channel.as_str(), link.channel_name.as_str()))
        .collect();
    let recent = recent
        .into_iter()
        .filter_map(|import| file(import, &names))
        .collect();
    Ok(SlackImports {
        store: files.name().to_owned(),
        unready,
        links: shown,
        recent,
    })
}

/// A file noticed, as the settings show it: none for one Slack doesn't host, as a canvas.
fn file(import: Import, channels: &BTreeMap<&str, &str>) -> Option<SlackImportedFile> {
    let status = match import.status {
        Status::Skipped => return None,
        Status::Waiting => SlackImportStatus::Waiting,
        Status::Saved => SlackImportStatus::Saved,
        Status::TooLarge => SlackImportStatus::TooLarge,
        Status::Gone => SlackImportStatus::Gone,
        Status::Failed => SlackImportStatus::Failed,
    };
    let described = import.described;
    Some(SlackImportedFile {
        channel_name: channels
            .get(import.channel.as_str())
            .map(|name| (*name).to_owned()),
        name: described.as_ref().map(|file| file.name.clone()),
        mime: described.as_ref().map(|file| file.mime.clone()),
        posted_by: described.and_then(|file| file.posted_name),
        retrying: import.status == Status::Waiting && import.attempts > 0,
        status,
        link: import.saved.map(|(_, link)| link),
        file_id: import.file,
        noticed_at: at(import.noticed_at),
    })
}

/// Links the channel `channel` Dispatch is in to the folder `folder`, or to a new folder named
/// after it, so its files from now on are saved there.
pub async fn link(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
    channel: String,
    folder: Option<String>,
) -> Result<SlackImports> {
    let files = store(state, &c, &team).await?;
    let dsp = c.dsp.id.clone();
    ensure(
        files.unready(state, &dsp).await?.is_none(),
        "slack_files_unready",
        409,
    )?;
    let bot = Bot::new(state, &dsp, &team);
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
        team: team.clone(),
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
            let linked = db.slack_import_link(&c.dsp.id, &link.team, &link.channel)?;
            ensure(linked.is_none(), "slack_channel_linked", 409)?;
            db.save_slack_import_link(&c.dsp.id, &link)?;
            record(db, &c, "slack.import_linked", &link)
        })
        .await?;
    overview(state, &c, &team).await
}

/// Saves the linked channel `channel`'s files to the folder `folder` from now on.
pub async fn move_to(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
    channel: String,
    folder: String,
) -> Result<SlackImports> {
    let files = store(state, &c, &team).await?;
    let folder = chosen(state, files, &c.dsp.id, &folder).await?;
    let (asked, of) = (c.clone(), team.clone());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let linked = db.slack_import_link(&c.dsp.id, &of, &channel)?;
            let mut link = linked.ok_or_else(|| Error::new("slack_channel_not_linked", 404))?;
            link.folder = folder.id;
            link.folder_name = folder.name;
            db.save_slack_import_link(&c.dsp.id, &link)?;
            record(db, &c, "slack.import_moved", &link)
        })
        .await?;
    overview(state, &c, &team).await
}

/// Stops importing the channel `channel`: what was waiting from it isn't saved.
pub async fn unlink(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
    channel: String,
) -> Result<SlackImports> {
    store(state, &c, &team).await?;
    let (asked, of) = (c.clone(), team.clone());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let linked = db.slack_import_link(&c.dsp.id, &of, &channel)?;
            let link = linked.ok_or_else(|| Error::new("slack_channel_not_linked", 404))?;
            db.remove_slack_import_link(&c.dsp.id, &of, &channel)?;
            record(db, &c, "slack.import_unlinked", &link)
        })
        .await?;
    overview(state, &c, &team).await
}

/// The folder `folder`, or the top of where the DSP keeps its files, as choosing a folder for
/// the workspace `team` browses it.
pub async fn browse(
    state: &Arc<State>,
    c: &Context,
    team: &str,
    folder: Option<String>,
) -> Result<SlackFolders> {
    let files = store(state, c, team).await?;
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

/// The folder `id`, chosen for a channel's files: one where the DSP keeps its files.
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
fn record(db: &dispatch_core::db::Store, c: &Context, action: &str, link: &Link) -> Result<()> {
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
