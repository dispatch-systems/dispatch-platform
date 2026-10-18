//! What Sync Canvases shows in a workspace's settings: where documents go and whether they can
//! be written now, its linked channels with how many canvases each keeps, and every canvas it
//! keeps, with how syncing it goes and where its document opens.
use super::{
    LINKING,
    storage::{Canvas, CanvasStore, Status},
};
use crate::{
    api::types::{SlackCanvas, SlackCanvasLink, SlackCanvasStatus, SlackCanvases},
    backend::channel_links::{self, storage::LinkStore},
};
use dispatch_core::{Result, State, accounts::Context, db::at};
use std::{collections::BTreeMap, sync::Arc};

/// Sync Canvases in the workspace `team`, for the member `c`.
pub async fn overview(state: &Arc<State>, c: &Context, team: &str) -> Result<SlackCanvases> {
    let files = channel_links::store(state, &LINKING, c, team).await?;
    let dsp = c.dsp.id.clone();
    let unready = files.unready(state, &dsp).await?;
    let (at_dsp, of) = (dsp.clone(), team.to_owned());
    let (links, picks, canvases) = state
        .read(move |db| {
            Ok((
                db.slack_channel_links(&at_dsp, &LINKING, &of)?,
                db.slack_canvas_picks(&at_dsp, &of)?,
                db.slack_canvases(&at_dsp, &of)?,
            ))
        })
        .await?;
    let ready = unready.is_none();
    let mut channels: BTreeMap<&str, &str> = BTreeMap::new();
    let mut shown_links = Vec::new();
    for link in &links {
        channels.insert(&link.channel, &link.channel_name);
        let (folder, folder_missing) =
            channel_links::folder_now(state, files, &dsp, ready, link.folder()).await?;
        let kept = canvases
            .iter()
            .filter(|canvas| canvas.channel == link.channel && canvas.doc.is_some())
            .filter(|canvas| !picks.iter().any(|pick| pick.canvas == canvas.canvas))
            .count();
        shown_links.push(SlackCanvasLink {
            channel_id: link.channel.clone(),
            channel_name: link.channel_name.clone(),
            private: link.private,
            folder,
            folder_missing,
            kept: kept as i64,
        });
    }
    let mut shown = Vec::new();
    for canvas in &canvases {
        let pick = picks.iter().find(|pick| pick.canvas == canvas.canvas);
        let chosen = match pick {
            Some(pick) => {
                let folder = (pick.folder.as_str(), pick.folder_name.as_str());
                Some(channel_links::folder_now(state, files, &dsp, ready, folder).await?)
            }
            None => None,
        };
        let channel_name = pick.map(|pick| pick.channel_name.clone()).or_else(|| {
            channels
                .get(canvas.channel.as_str())
                .map(|name| (*name).to_owned())
        });
        let title = canvas
            .title
            .clone()
            .or_else(|| pick.map(|pick| pick.title.clone()))
            .unwrap_or_else(|| "Canvas".to_owned());
        shown.push(shown_canvas(canvas, title, channel_name, chosen));
    }
    Ok(SlackCanvases {
        store: files.name().to_owned(),
        unready,
        links: shown_links,
        canvases: shown,
    })
}

/// A canvas kept, as the settings show it.
fn shown_canvas(
    canvas: &Canvas,
    title: String,
    channel_name: Option<String>,
    chosen: Option<(crate::api::types::SlackFolder, bool)>,
) -> SlackCanvas {
    let status = match canvas.status {
        Status::Waiting => SlackCanvasStatus::Waiting,
        Status::Synced => SlackCanvasStatus::Synced,
        Status::Gone => SlackCanvasStatus::Gone,
        Status::Failed => SlackCanvasStatus::Failed,
    };
    let (folder, folder_missing) = match chosen {
        Some((folder, missing)) => (Some(folder), missing),
        None => (None, false),
    };
    SlackCanvas {
        canvas_id: canvas.canvas.clone(),
        title,
        channel_name,
        folder,
        folder_missing,
        status,
        retrying: canvas.status == Status::Waiting && canvas.attempts > 0,
        link: canvas.doc.as_ref().map(|(_, link)| link.clone()),
        synced_at: canvas.synced_at.map(at),
    }
}
