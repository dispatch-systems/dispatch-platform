//! Slack's upkeep. Hourly, each working workspace of each active DSP with Slack on is asked
//! for, as the bot: that renews its token as it nears its end, so one nobody uses never lapses;
//! keeps the workspace's name and logo as Slack has them, and its members; and notices a
//! workspace whose tokens Slack stopped honouring, so the card says so and the DSP can reconnect
//! it before someone finds out by trying.
use super::{bot::Bot, logo, members, slack::Slack, storage::SlackStore, work};
use dispatch_core::{
    State,
    manifest::{Maintenance, Upkeep},
};
use futures_util::{StreamExt, stream};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: check,
};

/// How a failure is reported.
const FAILED: &str = "slack_check_failed";

/// Each pass reads in short database turns, and asks Slack about two workspaces at once at
/// most.
fn check(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due || Slack::of(&state.config).is_err() {
            return;
        }
        let mut working = Vec::new();
        for dsp in work::dsps(&state, FAILED).await {
            let at = dsp.clone();
            let found = state.read(move |db| db.slack_workspaces(&at)).await;
            match found {
                Ok(found) => working.extend(
                    found
                        .into_iter()
                        .filter(|workspace| !workspace.broken)
                        .map(|workspace| (dsp.clone(), workspace)),
                ),
                Err(error) => work::failed(FAILED, &error),
            }
        }
        stream::iter(working)
            .map(|(dsp, workspace)| {
                let state = state.clone();
                async move {
                    let bot = Bot::new(&state, &dsp, &workspace.team);
                    // Slack refusing the workspace's tokens breaks it as the call answers.
                    let info = bot.call("team.info", &json!({"team": bot.team})).await?;
                    if let Some(name) = info["team"]["name"].as_str()
                        && !name.is_empty()
                        && name != workspace.name
                    {
                        let name = name.to_owned();
                        let (dsp, team) = (dsp.clone(), workspace.team.clone());
                        state
                            .run(move |db| db.rename_slack_workspace(&dsp, &team, &name))
                            .await?;
                    }
                    if let Err(error) = logo::keep(&bot, &info["team"]["icon"]).await {
                        logo::failed(&error);
                    }
                    // Whatever Slack didn't tell of as it happened.
                    if let Err(error) = members::read(&bot).await {
                        members::failed(&error);
                    }
                    dispatch_core::Result::Ok(())
                }
            })
            .buffer_unordered(2)
            .for_each(|result| async move {
                match result {
                    Err(error) if error.code != "slack_connection_broken" => {
                        work::failed(FAILED, &error)
                    }
                    _ => {}
                }
            })
            .await;
    })
}
