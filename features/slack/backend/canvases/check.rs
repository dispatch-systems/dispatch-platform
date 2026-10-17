//! Every ten minutes, what Slack didn't tell of, as while the server was down: each linked
//! channel's canvases, as Slack lists them with when each was last edited, and each canvas
//! chosen on its own. A canvas new to Sync Canvases, or edited since its document was written,
//! is synced; one edited a moment ago waits a while, as whoever's typing may not have stopped.
use super::{
    ID, LINKING, queue,
    storage::{CanvasStore, Outcome, Pick, Status},
};
use crate::backend::{
    bot::Bot, channel_links::storage::LinkStore, connection, storage::SlackStore,
};
use dispatch_core::{
    Result, State,
    db::{Store, now},
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(10 * 60),
    run: pass,
};

/// The files each page of Slack's list holds, at most, and the pages read of one channel.
const PAGE: u64 = 100;
const PAGES: u64 = 10;

/// A canvas as Slack lists it: its ID, its title, and when it was last edited, in
/// milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    pub title: String,
    pub edited: i64,
}

/// The canvas Slack describes as `file`: none for any other file.
pub(super) fn listed(file: &Value) -> Option<Listed> {
    if file["filetype"] != "quip" {
        return None;
    }
    let id = file["id"].as_str()?.to_owned();
    let title = file["title"]
        .as_str()
        .or_else(|| file["name"].as_str())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Canvas")
        .to_owned();
    Some(Listed {
        id,
        title,
        edited: file["updated"].as_i64().unwrap_or(0) * 1000,
    })
}

/// The canvases in the channel `channel`, as Slack lists them, the newest first.
pub async fn canvases_in(bot: &Bot, channel: &str) -> Result<Vec<Listed>> {
    let mut found = Vec::new();
    for page in 1..=PAGES {
        let answer = bot
            .call(
                "files.list",
                &json!({"channel": channel, "types": "spaces", "count": PAGE, "page": page}),
            )
            .await?;
        found.extend(
            answer["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(listed),
        );
        if answer["paging"]["pages"].as_u64().unwrap_or(1) <= page {
            break;
        }
    }
    Ok(found)
}

/// Notes the canvas `listed`, found in `channel`: synced at once, or a while after it was last
/// edited, if that's a moment ago.
pub(super) fn note(
    db: &Store,
    dsp: &str,
    team: &str,
    channel: &str,
    listed: &Listed,
) -> Result<bool> {
    let at = now();
    let due = at.max(listed.edited + queue::QUIET_MS);
    db.note_slack_canvas(
        dsp,
        team,
        &listed.id,
        channel,
        &listed.title,
        listed.edited,
        at,
        due,
    )
}

fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let dsps = match state.read(|db| db.kept_dsps()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        for dsp in dsps {
            if let Err(error) = check(&state, &dsp).await {
                failed(&error);
            }
        }
    })
}

/// Notes the canvases the DSP's links and choices cover, in each working workspace with Sync
/// Canvases on, then syncs what's due.
pub(super) async fn check(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    let workspaces = state
        .read(move |db| {
            if !connection::open(db, &at)? {
                return Ok(Vec::new());
            }
            let mut on = Vec::new();
            for workspace in db.slack_workspaces(&at)? {
                let team = workspace.team;
                if !workspace.broken && db.slack_integrations(&at, &team)?.iter().any(|id| id == ID)
                {
                    let linked: BTreeSet<String> = db
                        .slack_channel_links(&at, &LINKING, &team)?
                        .into_iter()
                        .map(|link| link.channel)
                        .collect();
                    let picks = db.slack_canvas_picks(&at, &team)?;
                    on.push((team, linked, picks));
                }
            }
            Ok(on)
        })
        .await?;
    for (team, linked, picks) in workspaces {
        let bot = Bot::new(state, dsp, &team);
        let channels: BTreeSet<&String> = linked
            .iter()
            .chain(picks.iter().map(|pick| &pick.channel))
            .collect();
        // Each channel, and each canvas chosen, on its own: one Slack refuses, as a channel
        // archived or left, is logged and the rest are still noted.
        let mut seen = BTreeSet::new();
        for channel in channels {
            let covers = |listed: &Listed| {
                linked.contains(channel) || picks.iter().any(|pick| pick.canvas == listed.id)
            };
            match in_channel(state, &bot, channel, covers).await {
                Ok(noted) => seen.extend(noted),
                Err(error) => failed(&error),
            }
        }
        for pick in picks.iter().filter(|pick| !seen.contains(&pick.canvas)) {
            if let Err(error) = unlisted(state, &bot, pick).await {
                failed(&error);
            }
        }
    }
    queue::sync_due(state, dsp).await;
    Ok(())
}

/// Notes the canvases in `channel` that `covers` says are kept: the IDs of those noted.
async fn in_channel(
    state: &Arc<State>,
    bot: &Bot,
    channel: &str,
    covers: impl Fn(&Listed) -> bool,
) -> Result<Vec<String>> {
    let mut noted = Vec::new();
    for listed in canvases_in(bot, channel).await? {
        if covers(&listed) {
            noted.push(listed.id.clone());
            let (at, team, channel) = (bot.dsp.clone(), bot.team.clone(), channel.to_owned());
            state
                .run(move |db| note(db, &at, &team, &channel, &listed))
                .await?;
        }
    }
    Ok(noted)
}

/// Notes the canvas `pick`, chosen on its own, that its channel no longer lists, as Slack has
/// it now: gone, once Slack has it no more.
async fn unlisted(state: &Arc<State>, bot: &Bot, pick: &Pick) -> Result<()> {
    let found = match bot.call("files.info", &json!({"file": pick.canvas})).await {
        Err(error) if error.code == "slack_file_gone" => None,
        answer => listed(&answer?["file"]),
    };
    let (at, team, pick) = (bot.dsp.clone(), bot.team.clone(), pick.clone());
    state
        .run(move |db| match found {
            Some(listed) => note(db, &at, &team, &pick.channel, &listed).map(drop),
            None => {
                let gone = Outcome::Left {
                    status: Status::Gone,
                    reason: "slack_file_gone".into(),
                };
                db.finish_slack_canvas(&at, &team, &pick.canvas, &gone, now())
            }
        })
        .await
}

fn failed(error: &dispatch_core::Error) {
    observability::event(
        "error",
        "slack_canvas_check_failed",
        json!({"error": error.code}),
    );
}
