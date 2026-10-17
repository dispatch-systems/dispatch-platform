//! Each canvas to be synced, kept from when it's noticed until its document holds it as it was
//! last edited, so no edit is lost to a restart or a failure. A canvas new to Sync Canvases is
//! synced at once; one Slack tells of a change to waits until the changes stop for a while, or
//! ten minutes at most while they go on, so a document isn't written over at each keystroke.
//! Every minute the canvases due are synced; one that failed waits longer each time, and is
//! given up until it's edited again.
use super::{
    LINKING, check,
    storage::{Canvas, CanvasStore, Outcome},
    sync,
};
use crate::backend::{
    bot::Bot,
    channel_links::storage::LinkStore,
    work::{self, Queue},
};
use dispatch_core::{
    Result, State,
    db::{Store, now},
    manifest::{Maintenance, Upkeep, integrations::Heard},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// How a failure is reported.
const FAILED: &str = "slack_canvas_failed";
/// How long after the last change Slack told of a canvas is synced, in milliseconds: long
/// enough for whoever's typing to have stopped.
pub(super) const QUIET_MS: i64 = 2 * 60 * 1000;
/// The longest a canvas edited on and on waits, in milliseconds.
const LONGEST_MS: i64 = 10 * 60 * 1000;
/// How long a canvas nothing covers any more waits, should something cover it again.
const UNCOVERED_MS: i64 = 15 * 60 * 1000;

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: work::pass::<Canvases>,
};

/// What Sync Canvases does with what Slack told of: a change to a canvas it keeps waits for the
/// changes to stop; a canvas shared in a linked channel is synced at once.
pub fn heard(state: Arc<State>, heard: Heard) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = told(&state, &heard).await {
            work::failed(FAILED, &error);
        }
    })
}
async fn told(state: &Arc<State>, heard: &Heard) -> Result<()> {
    let Some(file) = heard.event["file_id"].as_str() else {
        return Ok(());
    };
    let (dsp, team, file) = (heard.dsp.clone(), heard.account.clone(), file.to_owned());
    match heard.kind.as_str() {
        "file_change" => {
            let at = now();
            state
                .run(move |db| {
                    db.heard_slack_canvas(&dsp, &team, &file, at, at + QUIET_MS, LONGEST_MS)
                })
                .await?;
        }
        "file_shared" => {
            let Some(channel) = heard.event["channel_id"].as_str().map(str::to_owned) else {
                return Ok(());
            };
            let (at, of, linked_in) = (dsp.clone(), team.clone(), channel.clone());
            let linked = state
                .read(move |db| db.slack_channel_link(&at, &LINKING, &of, &linked_in))
                .await?;
            if linked.is_none() {
                return Ok(());
            }
            let bot = Bot::new(state, &dsp, &team);
            let info = match bot.call("files.info", &json!({"file": file})).await {
                Err(error) if error.code == "slack_file_gone" => return Ok(()),
                answer => answer?,
            };
            let Some(listed) = check::listed(&info["file"]) else {
                return Ok(());
            };
            let at = dsp.clone();
            state
                .run(move |db| check::note(db, &at, &team, &channel, &listed))
                .await?;
            sync_due(state, &dsp).await;
        }
        _ => {}
    }
    Ok(())
}

/// Syncs the DSP's canvases that are due, each into the folder its choice names, or its
/// channel's link.
pub(super) async fn sync_due(state: &Arc<State>, dsp: &str) {
    work::due::<Canvases>(state, dsp).await;
}

/// The canvases to sync, as the pass works through them: claiming one finds where its document
/// goes, none while nothing covers it.
struct Canvases;
impl Queue for Canvases {
    type Item = Canvas;
    type Claimed = Option<String>;
    type Outcome = Outcome;
    const BATCH: usize = 10;
    const CLAIM_MS: i64 = 30 * 60 * 1000;
    const FAILED: &'static str = FAILED;

    fn due(db: &Store, dsp: &str, at: i64, most: usize) -> Result<Vec<Canvas>> {
        db.due_slack_canvases(dsp, at, most)
    }
    fn claim(
        db: &Store,
        dsp: &str,
        canvas: &Canvas,
        at: i64,
        until: i64,
    ) -> Result<Option<Option<String>>> {
        if !db.claim_slack_canvas(dsp, &canvas.team, &canvas.canvas, at, until)? {
            return Ok(None);
        }
        Ok(Some(folder(db, dsp, canvas)?))
    }
    async fn try_one(
        state: &Arc<State>,
        dsp: &str,
        canvas: &Canvas,
        folder: Option<String>,
    ) -> Outcome {
        match folder {
            Some(folder) => sync::one(state, dsp, canvas, &folder).await,
            None => Outcome::Later {
                reason: "slack_canvas_uncovered".to_owned(),
                at: now() + UNCOVERED_MS,
                counted: false,
            },
        }
    }
    fn finish(db: &Store, dsp: &str, canvas: &Canvas, outcome: &Outcome, at: i64) -> Result<()> {
        db.finish_slack_canvas(dsp, &canvas.team, &canvas.canvas, outcome, at)
    }
}

/// Where a document new to the canvas goes: the folder it was chosen for on its own, or else
/// its channel's; none while nothing covers it.
fn folder(db: &Store, dsp: &str, canvas: &Canvas) -> Result<Option<String>> {
    if let Some(pick) = db.slack_canvas_pick(dsp, &canvas.team, &canvas.canvas)? {
        return Ok(Some(pick.folder));
    }
    Ok(db
        .slack_channel_link(dsp, &LINKING, &canvas.team, &canvas.channel)?
        .map(|link| link.folder))
}

#[cfg(test)]
#[path = "../../tests/backend/canvases.rs"]
mod tests;
