//! Each workspace's logo, which Dispatch keeps a copy of to show, since the dashboard shows no
//! picture from elsewhere. Slack describes it in what `team.info` answers, which the hourly
//! check asks and which is asked as a workspace is connected, reconnected or updated: one size,
//! large enough for a mark on a sharp screen, downloaded only from Slack's own servers, and only
//! again once Slack gives it at another address. A workspace whose logo is Slack's own has none.
use super::{
    bot::{self, Bot},
    slack::Slack,
    storage::{Logo, SlackStore},
};
use dispatch_core::{
    Error, Result, State, db::Store, foundation::observability, manifest::people::Photo,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// The size kept, in pixels square: three times a mark of forty-odd.
const SIZE: &str = "image_132";
/// How large a picture is kept at most.
const MOST: usize = 256 * 1024;
/// What Slack's own servers are called, which pictures come from alone.
const HOSTS: &str = "slack-edge.com";
/// The kinds a picture may be, by how its bytes start.
const KINDS: &[(&[u8], &str)] = &[
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
];

/// The address `url` names, if a picture may come from there: Slack's own servers, over HTTPS.
pub fn address(url: &str) -> Option<url::Url> {
    let url = url::Url::parse(url).ok()?;
    let host = url.host_str()?;
    let slacks = host == HOSTS
        || host
            .strip_suffix(HOSTS)
            .is_some_and(|sub| sub.ends_with('.'));
    let plain = url.port().is_none() && url.username().is_empty() && url.password().is_none();
    (url.scheme() == "https" && slacks && plain).then_some(url)
}
/// The kind of picture `bytes` are, if one that's kept: a PNG, JPEG or GIF, and not too large.
pub fn kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() > MOST {
        return None;
    }
    KINDS
        .iter()
        .find(|(start, _)| bytes.starts_with(start))
        .map(|(_, kind)| *kind)
}

/// Keeps the workspace's logo as Slack describes it, `icon` in what `team.info` answers: none
/// for Slack's own, which forgets the one kept; else the picture of the size kept, downloaded
/// only when Slack gives it at another address than the one kept came from. Slack saying
/// nothing of it leaves the one kept, as does a picture that fails to come.
pub async fn keep(bot: &Bot, icon: &Value) -> Result<()> {
    let lock = bot::lock("logo", &bot.dsp, &bot.team);
    let _keeping = lock.lock().await;
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    if icon["image_default"] == true {
        return bot
            .state
            .run(move |db| db.drop_slack_logo(&dsp, &team))
            .await;
    }
    let Some(source) = icon[SIZE].as_str().map(str::to_owned) else {
        return Ok(());
    };
    let kept = bot.state.read(move |db| db.slack_logo(&dsp, &team)).await?;
    if kept.is_some_and(|kept| kept.source == source) {
        return Ok(());
    }
    let refused = || Error::new("slack_refused", 502);
    let url = address(&source).ok_or_else(refused)?;
    let picture = Slack::of(&bot.state.config)?.picture(&url, MOST).await?;
    kind(&picture).ok_or_else(refused)?;
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let logo = Logo { source, picture };
    bot.state
        .run(move |db| db.keep_slack_logo(&dsp, &team, &logo))
        .await
}
/// Reads the DSP's workspace `team`'s logo once it was connected, reconnected or updated, so it
/// shows before the hourly check. A failure waits for that check.
pub fn read_soon(state: &Arc<State>, dsp: &str, team: &str) {
    let bot = Bot::new(state, dsp, team);
    tokio::spawn(async move {
        let read = async {
            let info = bot.call("team.info", &json!({"team": bot.team})).await?;
            keep(&bot, &info["team"]["icon"]).await
        };
        if let Err(error) = read.await {
            failed(&error);
        }
    });
}
/// The workspace `team`'s logo, as kept, to show: its kind, and its picture.
pub fn picture(db: &Store, dsp: &str, team: &str) -> Result<Option<Photo>> {
    Ok(db
        .slack_logo(dsp, team)?
        .and_then(|logo| Some((kind(&logo.picture)?, logo.picture))))
}

pub(super) fn failed(error: &Error) {
    observability::event("warn", "slack_logo_failed", json!({"error": error.code}));
}

#[cfg(test)]
#[path = "../tests/backend/logo.rs"]
mod tests;
