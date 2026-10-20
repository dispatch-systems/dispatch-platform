//! What members wrote to the app while Dispatch couldn't hear Slack, as while the server was
//! down, kept as Slack would have told of it. As the server starts, and hourly, each direct
//! message the app has in each workspace where Communications works, once its admin has
//! approved reading them, is read (`im:history`) from when it was last read, or from when its
//! messages may first be kept, never earlier, with the replies since then in its threads. What
//! was written in the week before is read again for any edit or reply since. Slack lists the
//! latest first, so a direct message with more since then than a pass reads goes on, the next
//! pass, from the oldest message it read, and is read up to the first pass's time once it's back
//! down. Its direct messages are those Dispatch knows, and every other Slack lists the app as
//! having (`im:read`) once the workspace's admin has approved that.
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
/// What reading the app's direct messages, and finding those Dispatch doesn't know, need the
/// workspace's admin to have let it do.
const READING: &str = "im:history";
const LISTING: &str = "im:read";
/// The messages each page of Slack's holds, and the pages read of a conversation in one pass.
const PAGE: u64 = 200;
const PAGES: usize = 10;
/// How far before where it left off a direct message is read again, for edits and the replies in
/// threads begun before then, in milliseconds: a week.
const THREADS_MS: i64 = 7 * 24 * 60 * 60 * 1000;

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

/// A direct message the app has in a workspace: with whom, up to when it was read, and where
/// reading it goes on from, if a pass left more than it reads.
struct Direct {
    user: String,
    caught_up_at: Option<i64>,
    catching_up: Option<(i64, String)>,
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
                let granted = |scope: &str| workspace.scopes.iter().any(|had| had == scope);
                if !on.contains(&workspace.team) || !granted(READING) {
                    continue;
                }
                let Some(since) = incoming::kept_since(db, &at, &workspace.team)? else {
                    continue;
                };
                let known = db
                    .communications_in(&at, &workspace.team)?
                    .into_iter()
                    .map(|kept: Conversation| {
                        let direct = Direct {
                            user: kept.user,
                            caught_up_at: kept.caught_up_at,
                            catching_up: kept.catching_up,
                        };
                        (kept.channel, direct)
                    })
                    .collect();
                found.push(Workspace {
                    listing: granted(LISTING),
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
                    catching_up: None,
                });
            }
        }
        for (channel, direct) in directs {
            if direct.user == "USLACKBOT" {
                continue;
            }
            let from = direct.caught_up_at.unwrap_or(0).max(workspace.since);
            // Going on from where a pass left off, or reading up to now.
            let (to, before) = match direct.catching_up {
                Some((to, before)) => (to, Some(before)),
                None => (now(), None),
            };
            // What changed in the week before is read once, as a pass starts down, not again as
            // the next goes on.
            let read = async {
                if before.is_none() {
                    earlier(state, &bot, &channel, from).await?;
                }
                since(state, &bot, &channel, from, before.as_deref()).await
            };
            let left = match read.await {
                Ok(left) => left,
                Err(error) => {
                    kit::failed(FAILED, &error);
                    continue;
                }
            };
            let (at, team, user) = (dsp.to_owned(), workspace.team.clone(), direct.user);
            state
                .run_scoped(dsp, DOMAIN, move |db| {
                    let Some(kept) = db.communications_conversation_with(&at, &team, &user)? else {
                        return Ok(());
                    };
                    match left {
                        None => db.caught_up_communications(&at, &kept.id, to),
                        Some(before) => db.catching_up_communications(&at, &kept.id, to, &before),
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

/// Keeps what was written in the direct message `channel` since `from`, in milliseconds, before
/// the message Slack knows as `before` when a pass left off there, with the replies since then in
/// its threads: where the next pass goes on from, when there's more than a pass reads, which
/// Slack lists the latest first.
async fn since(
    state: &Arc<State>,
    bot: &Bot,
    channel: &str,
    from: i64,
    before: Option<&str>,
) -> Result<Option<String>> {
    let (oldest, mut cursor, mut last) = (incoming::ts(from), Value::Null, None);
    for _ in 0..PAGES {
        let page = bot
            .call(
                "conversations.history",
                &json!({"channel": channel, "oldest": oldest, "latest": before,
                    "inclusive": true, "limit": PAGE, "cursor": cursor}),
            )
            .await?;
        for message in page["messages"].as_array().into_iter().flatten() {
            incoming::keep(state, &bot.dsp, &bot.team, channel, message).await?;
            replied(state, bot, channel, message, from).await?;
            last = message["ts"].as_str().map(str::to_owned).or(last);
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => return Ok(None),
        };
    }
    Ok(last)
}
/// Keeps what changed since `from`, in milliseconds, of what was written in the direct message
/// `channel` in the week before: each edit, and each reply in its threads. A pass reads as much
/// of it as it reads of anything.
async fn earlier(state: &Arc<State>, bot: &Bot, channel: &str, from: i64) -> Result<()> {
    let (oldest, latest) = (incoming::ts(from - THREADS_MS), incoming::ts(from));
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let page = bot
            .call(
                "conversations.history",
                &json!({"channel": channel, "oldest": oldest, "latest": latest, "limit": PAGE,
                    "cursor": cursor}),
            )
            .await?;
        for message in page["messages"].as_array().into_iter().flatten() {
            let edited = message["edited"]["ts"].as_str().and_then(incoming::millis);
            if edited > Some(from) {
                incoming::keep(state, &bot.dsp, &bot.team, channel, message).await?;
            }
            replied(state, bot, channel, message, from).await?;
        }
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => return Ok(()),
        };
    }
    Ok(())
}
/// Keeps the replies since `from`, in milliseconds, in the thread `message` began, if it has any.
async fn replied(
    state: &Arc<State>,
    bot: &Bot,
    channel: &str,
    message: &Value,
    from: i64,
) -> Result<()> {
    let latest = message["latest_reply"].as_str().and_then(incoming::millis);
    match message["ts"].as_str() {
        Some(thread) if latest > Some(from) => {
            replies(state, bot, channel, thread, &incoming::ts(from)).await
        }
        _ => Ok(()),
    }
}
/// Keeps the replies in the thread `thread` of the direct message `channel` since `oldest`, as
/// many as a pass reads.
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
            _ => return Ok(()),
        };
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/backend/catch_up.rs"]
mod tests;
