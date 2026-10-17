//! The channels of a DSP's workspace Dispatch may be in, on the workspace's settings: its public
//! channels, which Dispatch joins and leaves itself, and the private ones someone invited the bot
//! to in Slack, which it may leave but not join again. Slack shows the bot only the channels it's
//! in, so being in one is all the access Dispatch has there.
use super::{bot::Bot, storage::SlackStore};
use crate::api::types::{SlackChannel, SlackChannels};
use dispatch_core::{Error, Result, State, accounts::Context, ensure, server::http::route::Dsp};
use serde_json::{Value, json};
use std::sync::Arc;

/// How many of Slack's pages of channels are read at most, each of up to 200.
const PAGES: usize = 10;

/// The bot in the DSP's workspace `team`, which must be one of its own and working, and the
/// workspace's name in Dispatch.
async fn bot(state: &Arc<State>, dsp: &str, team: &str) -> Result<(Bot, String)> {
    let (at, of) = (dsp.to_owned(), team.to_owned());
    let found = state.read(move |db| db.slack_workspace(&at, &of)).await?;
    let workspace = found.ok_or_else(|| Error::new("slack_workspace_not_found", 404))?;
    ensure(!workspace.broken, "slack_connection_broken", 409)?;
    let name = workspace.label.unwrap_or(workspace.name);
    Ok((Bot::new(state, dsp, team), name))
}

/// The workspace `team`'s channels Dispatch may be in, by name, with whether it's in each.
pub async fn list(state: &Arc<State>, c: &Context, team: &str) -> Result<SlackChannels> {
    let (bot, _) = bot(state, &c.dsp.id, team).await?;
    let mut channels = Vec::new();
    let mut cursor = Value::Null;
    for _ in 0..PAGES {
        let args = json!({
            "types": "public_channel,private_channel",
            "exclude_archived": true,
            "limit": 200,
            "cursor": cursor,
        });
        let page = bot.call("conversations.list", &args).await?;
        let listed = page["channels"].as_array().into_iter().flatten();
        channels.extend(listed.filter_map(described));
        cursor = match page["response_metadata"]["next_cursor"].as_str() {
            Some(next) if !next.is_empty() => json!(next),
            _ => break,
        };
    }
    channels.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(SlackChannels { channels })
}

/// Has Dispatch join the workspace `team`'s public channel `channel`.
pub async fn join(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
    channel: String,
) -> Result<SlackChannel> {
    let (bot, workspace) = bot(state, &c.dsp.id, &team).await?;
    let answer = bot
        .call("conversations.join", &json!({"channel": channel}))
        .await?;
    let joined = described(&answer["channel"]).ok_or_else(|| Error::new("slack_refused", 502))?;
    let joined = SlackChannel {
        member: true,
        ..joined
    };
    record(
        state,
        c,
        access,
        "slack.channel_joined",
        &joined,
        &workspace,
    )
    .await?;
    Ok(joined)
}

/// Has Dispatch leave the workspace `team`'s channel `channel`. A private one it can't join
/// again: someone in it invites it back in Slack.
pub async fn leave(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    team: String,
    channel: String,
) -> Result<SlackChannel> {
    let (bot, workspace) = bot(state, &c.dsp.id, &team).await?;
    let answer = bot
        .call("conversations.info", &json!({"channel": channel}))
        .await?;
    let left = described(&answer["channel"]).ok_or_else(|| Error::new("slack_refused", 502))?;
    bot.call("conversations.leave", &json!({"channel": channel}))
        .await?;
    let left = SlackChannel {
        member: false,
        ..left
    };
    record(state, c, access, "slack.channel_left", &left, &workspace).await?;
    Ok(left)
}

/// A channel as Slack describes it to the bot.
pub(crate) fn described(channel: &Value) -> Option<SlackChannel> {
    Some(SlackChannel {
        id: channel["id"].as_str()?.to_owned(),
        name: channel["name"].as_str()?.to_owned(),
        private: channel["is_private"] == true,
        member: channel["is_member"] == true,
    })
}

/// Records Dispatch joining or leaving a channel in the activity log, once the member is
/// checked again: `#<channel> in <workspace>`.
async fn record(
    state: &Arc<State>,
    c: Context,
    access: Dsp,
    action: &'static str,
    channel: &SlackChannel,
    workspace: &str,
) -> Result<()> {
    let detail = format!("#{} in {workspace}", channel.name);
    state
        .run(move |db| {
            let c = access.revalidate(db, &c)?;
            c.audit(db, action, &detail)
        })
        .await
}

/// Whether `channel` is a Slack channel's ID as Slack writes them: a public one's, or a private
/// one's.
pub fn channel_id(channel: &str) -> bool {
    (2..=32).contains(&channel.len())
        && channel.starts_with(['C', 'G'])
        && channel
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
