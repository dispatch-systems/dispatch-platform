//! What members write to the app directly, kept as Slack tells of it (`message.im`) or as
//! catching up reads it: each message a person wrote anew there, shared a file in, or wrote in a
//! thread of, with its files to download, and each later edit of one, its text replaced and
//! nothing of what it said before kept. Never the app's own posts or another bot's, and nothing
//! deleted in Slack is deleted here. Never one sent before Slack, Communications and its
//! integration in the workspace were all on.
use super::ID;
use crate::backend::{
    files, live,
    storage::{Attached, CommunicationsStore, DOMAIN, Written},
    text,
};
use dispatch_core::{Result, State, db::Store};
use dispatch_slack::kit::{self, Bot};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

/// The time Slack's `ts` names, in milliseconds: seconds, a point, then microseconds.
pub fn millis(ts: &str) -> Option<i64> {
    let (seconds, micros) = ts.split_once('.').unwrap_or((ts, "0"));
    let micros = format!("{micros:0<6}");
    Some(seconds.parse::<i64>().ok()? * 1000 + micros.get(..3)?.parse::<i64>().ok()?)
}
/// Slack's `ts` for the time `at`, in milliseconds.
pub fn ts(at: i64) -> String {
    format!("{}.{:03}000", at.div_euclid(1000), at.rem_euclid(1000))
}

/// From when the DSP's messages in the workspace `team` may be kept, in milliseconds: since
/// Slack, Communications and its integration there were all on. None while any is off.
pub fn kept_since(db: &Store, dsp: &str, team: &str) -> Result<Option<i64>> {
    let on = [
        db.feature_on_since(dsp, "slack")?,
        db.feature_on_since(dsp, crate::FEATURE.name)?,
        kit::integration_on_since(db, dsp, team, ID)?,
    ];
    Ok(on
        .into_iter()
        .try_fold(0, |since, on| on.map(|on| since.max(on))))
}

/// Keeps what Slack told of in the workspace `team`: a message a member wrote, or an edit of one.
pub async fn heard(state: &Arc<State>, dsp: &str, team: &str, event: &Value) -> Result<()> {
    match event["subtype"].as_str() {
        Some("message_changed") => edited(state, dsp, team, &event["message"]).await,
        _ => {
            let channel = event["channel"].as_str().unwrap_or_default();
            keep(state, dsp, team, channel, event).await.map(drop)
        }
    }
}

/// Whether `message`, as Slack describes one, is one a person wrote anew, shared a file in, or
/// wrote in a thread and sent to the conversation too: from no bot, as the app's own posts are,
/// and with no other subtype, as an edit or a deletion has.
fn written(message: &Value) -> bool {
    let person = message["user"]
        .as_str()
        .is_some_and(|user| !user.is_empty() && user != "USLACKBOT");
    let kind = message["subtype"].as_str();
    message["bot_id"].is_null()
        && person
        && matches!(kind, None | Some("file_share" | "thread_broadcast"))
}

/// Keeps `message`, written in the workspace `team`'s direct message `channel`, unless it's
/// another's or sent before its messages may be kept, then downloads its files: whether it
/// changed anything.
pub async fn keep(
    state: &Arc<State>,
    dsp: &str,
    team: &str,
    channel: &str,
    message: &Value,
) -> Result<bool> {
    let (Some(user), Some(ts)) = (message["user"].as_str(), message["ts"].as_str()) else {
        return Ok(false);
    };
    let Some(sent) = millis(ts).filter(|_| written(message) && !channel.is_empty()) else {
        return Ok(false);
    };
    let (at, of) = (dsp.to_owned(), team.to_owned());
    let (since, names) = state
        .read(move |db| Ok((kept_since(db, &at, &of)?, names(db, &at, &of)?)))
        .await?;
    if since.is_none_or(|since| sent < since) {
        return Ok(false);
    }
    let name = match names.get(user) {
        Some(name) => name.clone(),
        None => asked(state, dsp, team, user).await,
    };
    let files = message["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            Some(Attached {
                slack_id: file["id"].as_str()?.to_owned(),
                name: file["name"]
                    .as_str()
                    .or(file["title"].as_str())
                    .unwrap_or("file")
                    .to_owned(),
                mime: file["mimetype"]
                    .as_str()
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
                size: file["size"].as_i64().unwrap_or(0),
            })
        })
        .collect();
    let written = Written {
        team: team.to_owned(),
        user: user.to_owned(),
        channel: channel.to_owned(),
        name,
        ts: ts.to_owned(),
        thread_ts: message["thread_ts"].as_str().map(str::to_owned),
        text: text::plain(message["text"].as_str().unwrap_or_default(), |id| {
            names.get(id).cloned()
        }),
        at: sent,
        edited_at: message["edited"]["ts"].as_str().and_then(millis),
        files,
    };
    let of = dsp.to_owned();
    let kept = state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.keep_communications_message(&of, &written)
        })
        .await?;
    let Some(kept) = kept else {
        return Ok(false);
    };
    live::changed(state, dsp);
    if !kept.files.is_empty() {
        let (state, dsp) = (state.clone(), dsp.to_owned());
        tokio::spawn(async move { files::download_due(&state, &dsp).await });
    }
    Ok(true)
}

/// Replaces the text of a message kept with the edit `message` describes, from the workspace
/// `team`. Only its latest text is kept: what it said before is neither read nor logged.
async fn edited(state: &Arc<State>, dsp: &str, team: &str, message: &Value) -> Result<()> {
    let (Some(user), Some(ts), Some(edited_at)) = (
        message["user"].as_str(),
        message["ts"].as_str(),
        message["edited"]["ts"].as_str().and_then(millis),
    ) else {
        // Slack changing a message itself, as to show a link's preview, isn't an edit.
        return Ok(());
    };
    if !message["bot_id"].is_null() {
        return Ok(());
    }
    let (at, of) = (dsp.to_owned(), team.to_owned());
    let names = state.read(move |db| names(db, &at, &of)).await?;
    let text = text::plain(message["text"].as_str().unwrap_or_default(), |id| {
        names.get(id).cloned()
    });
    let (of, team, user, ts) = (
        dsp.to_owned(),
        team.to_owned(),
        user.to_owned(),
        ts.to_owned(),
    );
    let changed = state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.edit_communications_message(&of, &team, &user, &ts, &text, edited_at)
        })
        .await?;
    if changed.is_some() {
        live::changed(state, dsp);
    }
    Ok(())
}

/// The workspace `team`'s members' names, by their IDs, as Slack's members were last read.
fn names(db: &Store, dsp: &str, team: &str) -> Result<HashMap<String, String>> {
    Ok(kit::members(db, dsp)?
        .into_iter()
        .filter(|(of, _)| of == team)
        .map(|(_, member)| (member.user, member.name))
        .collect())
}
/// The workspace `team`'s member `user`'s name, as Slack describes them now, for one who joined
/// since its members were read: their ID, if Slack doesn't say.
async fn asked(state: &Arc<State>, dsp: &str, team: &str, user: &str) -> String {
    let info = Bot::new(state, dsp, team)
        .call("users.info", &json!({"user": user}))
        .await;
    info.ok()
        .and_then(|info| kit::member(&info["user"]))
        .map_or_else(|| user.to_owned(), |member| member.name)
}

#[cfg(test)]
#[path = "../../tests/backend/incoming.rs"]
mod tests;
