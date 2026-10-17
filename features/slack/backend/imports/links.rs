//! What Import Files shows in a workspace's settings: where files go and whether they can be
//! saved now, its linked channels with what each saved, and the files noticed latest. Its
//! channels are linked, moved and unlinked by `channel_links`, as `LINKING` declares.
use super::{
    LINKING,
    storage::{Import, ImportStore, Status},
};
use crate::{
    api::types::{SlackImportLink, SlackImportStatus, SlackImportedFile, SlackImports},
    backend::channel_links::{self, storage::LinkStore},
};
use dispatch_core::{Result, State, accounts::Context, db::at};
use std::{collections::BTreeMap, sync::Arc};

/// The files noticed latest the settings show.
const RECENT: usize = 20;

/// Import Files in the workspace `team`: where files go and whether they can be saved now, its
/// linked channels with their folders as they are now and what each saved, and the files
/// noticed latest.
pub async fn overview(state: &Arc<State>, c: &Context, team: &str) -> Result<SlackImports> {
    let files = channel_links::store(state, &LINKING, c, team).await?;
    let dsp = c.dsp.id.clone();
    let unready = files.unready(state, &dsp).await?;
    let (at_dsp, of) = (dsp.clone(), team.to_owned());
    let (links, totals, recent) = state
        .read(move |db| {
            Ok((
                db.slack_channel_links(&at_dsp, &LINKING, &of)?,
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
        let (folder, folder_missing) =
            channel_links::folder_now(state, files, &dsp, unready.is_none(), link).await?;
        let (saved, last) = totals.get(&link.channel).copied().unwrap_or((0, None));
        shown.push(SlackImportLink {
            channel_id: link.channel.clone(),
            channel_name: link.channel_name.clone(),
            private: link.private,
            folder,
            folder_missing,
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
