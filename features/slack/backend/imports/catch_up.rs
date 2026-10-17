//! Hourly, the files Slack didn't tell of, as while the server was down: each linked channel's
//! files since it was linked, as Slack lists them, are noticed like those it told of. A file
//! noticed already is noticed once still.
use super::{ID, LINKING, queue};
use crate::backend::{bot::Bot, channel_links::storage::LinkStore, work};
use dispatch_core::{
    Result, State,
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};

/// How a failure is reported.
const FAILED: &str = "slack_import_catch_up_failed";
/// The files each page of Slack's list holds, at most.
const PAGE: u64 = 100;

fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        for dsp in work::dsps(&state, FAILED).await {
            if let Err(error) = catch_up(&state, &dsp).await {
                work::failed(FAILED, &error);
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
            let mut on = Vec::new();
            for team in work::workspaces_on(db, &at, ID)? {
                let links = db.slack_channel_links(&at, &LINKING, &team)?;
                on.push((team, links));
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
