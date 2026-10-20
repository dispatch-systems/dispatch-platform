//! What members wrote to the app while Dispatch couldn't hear Slack, as while the server was
//! down, kept as Slack would have told of it. As the server starts, and hourly, each direct
//! message the app has in each workspace where Communications works is read (`im:history`) from
//! when it was last read, or from when its messages may first be kept, never earlier, with the
//! threads in it. Its direct messages are those Dispatch knows, and every other Slack lists the
//! app as having (`im:read`) once the workspace's admin has approved that.
use super::{ID, incoming};
use crate::backend::storage::{CommunicationsStore, Conversation, DOMAIN};
use dispatch_core::{
    Result, State,
    db::now,
    manifest::{Maintenance, Upkeep},
};
use dispatch_slack::kit::{self, Bot};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};

/// How a failure is reported.
const FAILED: &str = "communications_catch_up_failed";
/// What finding the app's direct messages Dispatch doesn't know needs the workspace's admin to
/// have let it do.
const LISTING: &str = "im:read";
/// The messages each page of Slack's holds, and the pages read of a conversation in one pass.
const PAGE: u64 = 200;
const PAGES: usize = 10;

fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        for dsp in kit::dsps(&state, FAILED).await {
            if let Err(error) = catch_up(&state, &dsp).await {
                kit::failed(FAILED, &error);
            }
        }
    })
}

/// A direct message the app has in a workspace: with whom, and up to when it was read.
struct Direct {
    user: String,
    caught_up_at: Option<i64>,
}
/// What catching up a workspace starts from: from when its messages may be kept, whether the
/// app may list its direct messages, and those Dispatch knows, by their IDs.
struct Workspace {
    team: String,
    since: i64,
    listing: bool,
    known: BTreeMap<String, Direct>,
}

/// Keeps what the DSP's members wrote to the app that Dispatch didn't hear of, in each workspace
/// where Communications works while it's on for the DSP.
pub async fn catch_up(state: &Arc<State>, dsp: &str) -> Result<()> {
    let at = dsp.to_owned();
    let workspaces = state
        .read(move |db| {
            if !db.feature_enabled(&at, crate::FEATURE.name)? {
                return Ok(Vec::new());
            }
            let on = kit::workspaces_on(db, &at, ID)?;
            let mut found = Vec::new();
            for workspace in kit::workspaces(db, &at)? {
                if !on.contains(&workspace.team) {
                    continue;
                }
                let Some(since) = incoming::kept_since(db, &at, &workspace.team)? else {
                    continue;
                };
                let known = db
                    .communications_in(&at, &workspace.team)?
                    .into_iter()
                    .filter_map(|Conversation { channel, user, caught_up_at, .. }| {
                        Some((channel?, Direct { user, caught_up_at }))
                    })
                    .collect();
                found.push(Workspace {
                    listing: workspace.scopes.iter().any(|scope| scope == LISTING),
                    team: workspace.team,
                    since,
                    known,
                });
            }
            Ok(found)
        })
        .await?;
    for workspace in workspaces {
        let bot = Bot::new(state, dsp, &workspace.team);
        let mut directs = workspace.known;
        if workspace.listing {
            for (channel, user) in listed(&bot).await? {
                directs.entry(channel).or_insert(Direct {
                    user,
                    caught_up_at: None,
                });
            }
        }
        for (channel, direct) in directs {
            if direct.user == "USLACKBOT" {
                continue;
            }
            let from = direct.caught_up_at.unwrap_or(0).max(workspace.since);
            let started = now();
            if let Err(error) = read(state, &bot, &channel, from).await {
                kit::failed(FAILED, &error);
                continue;
            }
            let (at, team, user) = (dsp.to_owned(), workspace.team.clone(), direct.user);
            state
                .run_scoped(dsp, DOMAIN, move |db| {
                    match db.communications_conversation_with(&at, &team, &user)? {
                        Some(kept) => db.caught_up_communications(&at, &kept.id, started),
                        None => Ok(()),
                    }
                })
                .await?;
        }
    }
    Ok(())
}

/// The direct messages Slack lists the app as having in the bot's workspace, each with whom.
async fn listed(bot: &Bot) -> Result<Vec<(String, String)>> {
    let mut found = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = bot
            .call(
                "users.conversations",
                &json!({"types": "im", "exclude_archived": true, "limit": PAGE, "cursor": cursor}),
            )
            .await?;
        for channel in page["channels"].as_array().into_iter().flatten() {
            if let (Some(id), Some(user)) = (channel["id"].as_str(), channel["user"].as_str()) {
                found.push((id.to_owned(), user.to_owned()));
            }
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => break,
        };
    }
    Ok(found)
}

/// Keeps what was written in the direct message `channel` since `from`, in milliseconds, with
/// the replies in its threads since then.
async fn read(state: &Arc<State>, bot: &Bot, channel: &str, from: i64) -> Result<()> {
    let oldest = incoming::ts(from);
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = bot
            .call(
                "conversations.history",
                &json!({"channel": channel, "oldest": oldest, "limit": PAGE, "cursor": cursor}),
            )
            .await?;
        for message in page["messages"].as_array().into_iter().flatten() {
            incoming::keep(state, &bot.dsp, &bot.team, channel, message).await?;
            let replied = message["latest_reply"].as_str().and_then(incoming::millis);
            if let (Some(thread), true) = (
                message["ts"].as_str(),
                replied.is_some_and(|latest| latest > from),
            ) {
                replies(state, bot, channel, thread, &oldest).await?;
            }
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => break,
        };
    }
    Ok(())
}
/// Keeps the replies in the thread `thread` of the direct message `channel` since `oldest`.
async fn replies(
    state: &Arc<State>,
    bot: &Bot,
    channel: &str,
    thread: &str,
    oldest: &str,
) -> Result<()> {
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = bot
            .call(
                "conversations.replies",
                &json!({"channel": channel, "ts": thread, "oldest": oldest, "limit": PAGE,
                    "cursor": cursor}),
            )
            .await?;
        for message in page["messages"].as_array().into_iter().flatten() {
            if message["ts"].as_str() != Some(thread) {
                incoming::keep(state, &bot.dsp, &bot.team, channel, message).await?;
            }
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => break,
        };
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/catch_up.rs"]
mod tests;
