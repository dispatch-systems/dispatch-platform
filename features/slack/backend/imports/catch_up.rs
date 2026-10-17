//! Hourly, the files Slack didn't tell of, as while the server was down: each linked channel's
//! files since it was linked, as Slack lists them, are noticed like those it told of. A file
//! noticed already is noticed once still.
use super::{ID, queue, storage::ImportStore};
use crate::backend::{bot::Bot, connection, storage::SlackStore};
use dispatch_core::{
    Result, State,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};

/// The files each page of Slack's list holds, at most.
const PAGE: u64 = 100;

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
            if let Err(error) = catch_up(&state, &dsp).await {
                failed(&error);
            }
        }
    })
}

/// Notices the DSP's linked channels' files Slack didn't tell of, in each working workspace
/// with Import Files on, then saves what's due.
pub(super) async fn catch_up(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    let workspaces = state
        .read(move |db| {
            if !connection::open(db, &at)? {
                return Ok(Vec::new());
            }
            let mut on = Vec::new();
            for workspace in db.slack_workspaces(&at)? {
                if !workspace.broken
                    && db
                        .slack_integrations(&at, &workspace.team)?
                        .iter()
                        .any(|id| id == ID)
                {
                    let links = db.slack_import_links(&at, &workspace.team)?;
                    on.push((workspace.team, links));
                }
            }
            Ok(on)
        })
        .await?;
    for (team, links) in workspaces {
        let bot = Bot::new(state, dsp, &team);
        for link in links {
            let listed = bot
                .call(
                    "files.list",
                    &json!({"channel": link.channel, "ts_from": link.linked_at / 1000,
                        "count": PAGE}),
                )
                .await?;
            for file in listed["files"].as_array().into_iter().flatten() {
                let (Some(id), Some(created)) = (file["id"].as_str(), file["created"].as_i64())
                else {
                    continue;
                };
                let (at, team, id, channel) = (
                    dsp.to_owned(),
                    team.clone(),
                    id.to_owned(),
                    link.channel.clone(),
                );
                state
                    .run(move |db| queue::notice(db, &at, &team, &id, &channel, created * 1000))
                    .await?;
            }
        }
    }
    queue::save_due(state, dsp).await;
    Ok(())
}

fn failed(error: &dispatch_core::Error) {
    observability::event(
        "error",
        "slack_import_catch_up_failed",
        json!({"error": error.code}),
    );
}
