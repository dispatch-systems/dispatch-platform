//! The canvases a DSP chooses on their own, from a channel Dispatch is in, each kept in a
//! folder; and a channel's canvases to choose from. For a member who manages Slack and sees the
//! folders where the DSP keeps its files, while Sync Canvases is on in the workspace. A canvas
//! chosen is synced at once; the activity log records each choice, and each choice undone.
use super::{
    LINKING, check, queue,
    storage::{CanvasStore, Pick},
};
use crate::{
    api::types::{SlackChannelCanvas, SlackChannelCanvases},
    backend::{
        bot::Bot,
        channel_links::{self, storage::LinkStore},
    },
};
use dispatch_core::{
    Error, Result, State, accounts::Context, db::now, ensure, server::http::route::Dsp,
};
use std::sync::Arc;

/// The canvases in the channel `channel` Dispatch is in, to choose from: each by its title,
/// and whether it's chosen already; and whether the channel is linked, so they all are.
pub async fn of_channel(
    state: &Arc<State>,
    c: &Context,
    team: &str,
    channel: &str,
) -> Result<SlackChannelCanvases> {
    channel_links::store(state, &LINKING, c, team).await?;
    let bot = Bot::new(state, &c.dsp.id, team);
    let seen = channel_links::joined(&bot, channel).await?;
    let listed = check::canvases_in(&bot, &seen.id).await?;
    let (dsp, at, of) = (c.dsp.id.clone(), team.to_owned(), seen.id.clone());
    let (picks, linked) = state
        .read(move |db| {
            Ok((
                db.slack_canvas_picks(&dsp, &at)?,
                db.slack_channel_link(&dsp, &LINKING, &at, &of)?.is_some(),
            ))
        })
        .await?;
    Ok(SlackChannelCanvases {
        linked,
        canvases: listed
            .into_iter()
            .map(|listed| SlackChannelCanvas {
                chosen: picks.iter().any(|pick| pick.canvas == listed.id),
                id: listed.id,
                title: listed.title,
            })
            .collect(),
    })
}

/// Chooses the canvases `canvases` of the channel `channel` Dispatch is in, to keep in the
/// folder `folder`, or a new one named after the channel, and syncs them. One chosen already
/// stays where it was chosen for.
#[allow(clippy::too_many_arguments)]
pub async fn choose(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    team: &str,
    channel: &str,
    canvases: Vec<String>,
    folder: Option<String>,
) -> Result<()> {
    let files = channel_links::store(state, &LINKING, c, team).await?;
    let dsp = c.dsp.id.clone();
    ensure(
        files.unready(state, &dsp).await?.is_none(),
        "slack_files_unready",
        409,
    )?;
    let bot = Bot::new(state, &dsp, team);
    let seen = channel_links::joined(&bot, channel).await?;
    let listed = check::canvases_in(&bot, &seen.id).await?;
    let mut chosen = Vec::new();
    for id in canvases {
        let found = listed.iter().find(|listed| listed.id == id);
        chosen.push(
            found
                .ok_or_else(|| Error::new("slack_canvas_not_found", 404))?
                .clone(),
        );
    }
    ensure(!chosen.is_empty(), "invalid_input", 400)?;
    let folder = channel_links::folder_for(state, files, c, folder, &seen.name).await?;
    let (asked, at) = (c.clone(), team.to_owned());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let workspace = channel_links::workspace(db, &c.dsp.id, &at)?;
            for listed in chosen {
                if db.slack_canvas_pick(&c.dsp.id, &at, &listed.id)?.is_some() {
                    continue;
                }
                let pick = Pick {
                    team: at.clone(),
                    canvas: listed.id.clone(),
                    title: listed.title.clone(),
                    channel: seen.id.clone(),
                    channel_name: seen.name.clone(),
                    private: seen.private,
                    folder: folder.id.clone(),
                    folder_name: folder.name.clone(),
                    picked_by: c.actor().to_owned(),
                    picked_at: now(),
                };
                db.save_slack_canvas_pick(&c.dsp.id, &pick)?;
                check::note(db, &c.dsp.id, &at, &seen.id, &listed)?;
                let detail = format!(
                    "{} in #{} to {} in {workspace}",
                    pick.title, pick.channel_name, pick.folder_name
                );
                c.audit(db, "slack.canvas_chosen", &detail)?;
            }
            Ok(())
        })
        .await?;
    synced_soon(state, &dsp);
    Ok(())
}

/// Stops syncing the canvas `canvas` chosen on its own: its document stays where it is.
pub async fn unchoose(
    state: &Arc<State>,
    c: &Context,
    access: Dsp,
    team: &str,
    canvas: String,
) -> Result<()> {
    channel_links::store(state, &LINKING, c, team).await?;
    let (asked, at) = (c.clone(), team.to_owned());
    state
        .run(move |db| {
            let c = access.revalidate(db, &asked)?;
            let pick = db
                .slack_canvas_pick(&c.dsp.id, &at, &canvas)?
                .ok_or_else(|| Error::new("slack_canvas_not_chosen", 404))?;
            db.remove_slack_canvas_pick(&c.dsp.id, &at, &canvas)?;
            let detail = format!(
                "{} in #{} in {}",
                pick.title,
                pick.channel_name,
                channel_links::workspace(db, &c.dsp.id, &at)?
            );
            c.audit(db, "slack.canvas_unchosen", &detail)
        })
        .await
}

/// Notes the canvases of the channel `channel`, just linked, and syncs them.
pub async fn linked(state: &Arc<State>, dsp: &str, team: &str, channel: &str) -> Result<()> {
    let listed = check::canvases_in(&Bot::new(state, dsp, team), channel).await?;
    let (at, of, linked_in) = (dsp.to_owned(), team.to_owned(), channel.to_owned());
    state
        .run(move |db| {
            for listed in &listed {
                check::note(db, &at, &of, &linked_in, listed)?;
            }
            Ok(())
        })
        .await?;
    synced_soon(state, dsp);
    Ok(())
}

/// Syncs what's due for the DSP, without waiting for it.
fn synced_soon(state: &Arc<State>, dsp: &str) {
    let (state, dsp) = (state.clone(), dsp.to_owned());
    tokio::spawn(async move { queue::sync_due(&state, &dsp).await });
}
