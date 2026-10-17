//! The members of the DSP's workspaces, for Driver Match to tell apart: each one's name, the
//! email address and phone number on their profile, and whether their account is active. A
//! workspace's are read whole when it's connected, or its admin approves reading them, and
//! hourly; one at a time as Slack tells of someone joining or changing their profile. Bots and
//! Slackbot aren't members here; guests are.
use super::{
    bot::{self, Bot},
    storage::{Member, SlackStore},
};
use dispatch_core::{Error, Result, State, foundation::observability, manifest::people};
use serde_json::{Value, json};
use std::sync::Arc;

/// What reading members needs the workspace's admin to have let the bot do.
pub const SCOPES: &[&str] = &["users:read", "users:read.email"];
/// How many of Slack's pages of members are read at most, each of up to 200.
const PAGES: usize = 50;

/// A member as Slack describes a user, unless they're a bot or Slackbot.
pub(super) fn member(user: &Value) -> Option<Member> {
    let id = user["id"].as_str().filter(|id| !id.is_empty())?;
    if id == "USLACKBOT" || user["is_bot"] == true || user["is_app_user"] == true {
        return None;
    }
    let text = |value: &Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let profile = &user["profile"];
    // Their username only when their profile gives no name at all.
    let name = text(&profile["real_name"])
        .or_else(|| text(&user["real_name"]))
        .or_else(|| text(&profile["display_name"]))
        .or_else(|| text(&user["name"]))?;
    Some(Member {
        user: id.to_owned(),
        display_name: text(&profile["display_name"]).filter(|shown| *shown != name),
        name,
        email: text(&profile["email"]),
        phone: text(&profile["phone"]),
        active: user["deleted"] != true,
        guest: user["is_restricted"] == true || user["is_ultra_restricted"] == true,
    })
}

/// Whether the workspace's admin let the bot read its members.
async fn readable(bot: &Bot) -> Result<bool> {
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let workspace = bot
        .state
        .read(move |db| db.slack_workspace(&dsp, &team))
        .await?;
    Ok(workspace.is_some_and(|workspace| {
        SCOPES
            .iter()
            .all(|scope| workspace.scopes.iter().any(|granted| granted == scope))
    }))
}

/// Reads the workspace's members whole and keeps them; when any is new, changed or gone,
/// Driver Match tells them apart at once. A member Slack tells of meanwhile is kept after it,
/// so the list read before doesn't undo them.
pub async fn read(bot: &Bot) -> Result<()> {
    let lock = bot::lock("members", &bot.dsp, &bot.team);
    let _keeping = lock.lock().await;
    if !readable(bot).await? {
        return Ok(());
    }
    let mut members = Vec::new();
    let mut cursor = Value::Null;
    let mut whole = false;
    for _ in 0..PAGES {
        let page = bot
            .call("users.list", &json!({"limit": 200, "cursor": cursor}))
            .await?;
        let listed = page["members"].as_array().into_iter().flatten();
        members.extend(listed.filter_map(member));
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => {
                whole = true;
                break;
            }
        };
    }
    // Part of a list would have the rest gone.
    if !whole {
        return Err(Error::new("slack_members_too_many", 502));
    }
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let changed = bot
        .state
        .run(move |db| db.keep_slack_members(&dsp, &team, &members))
        .await?;
    if changed {
        people::changed(bot.state.clone(), bot.dsp.clone()).await;
    }
    Ok(())
}
/// Reads the DSP's workspace `team`'s members once Slack has given the bot its access there,
/// as it was just connected or its admin approved reading them. A failure waits for the
/// hourly check.
pub fn read_soon(state: &Arc<State>, dsp: &str, team: &str) {
    let bot = Bot::new(state, dsp, team);
    tokio::spawn(async move {
        if let Err(error) = read(&bot).await {
            failed(&error);
        }
    });
}

/// Keeps the member Slack told of, who joined the workspace or changed their profile; when
/// anything Driver Match reads changed, it tells them apart at once.
pub async fn told(bot: &Bot, event: &Value) -> Result<()> {
    let Some(member) = member(&event["user"]) else {
        return Ok(());
    };
    let lock = bot::lock("members", &bot.dsp, &bot.team);
    let _keeping = lock.lock().await;
    if !readable(bot).await? {
        return Ok(());
    }
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let changed = bot
        .state
        .run(move |db| db.keep_slack_member(&dsp, &team, &member))
        .await?;
    if changed {
        people::changed(bot.state.clone(), bot.dsp.clone()).await;
    }
    Ok(())
}

pub(super) fn failed(error: &Error) {
    observability::event("warn", "slack_members_failed", json!({"error": error.code}));
}

#[cfg(test)]
#[path = "../tests/backend/members.rs"]
mod tests;
