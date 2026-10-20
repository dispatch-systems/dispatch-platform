//! Fixture mode's answers to the Slack methods only Communications calls, so previews and tests
//! never reach Slack. The app has a direct message with each member, `D` and the rest of their
//! ID (`D0FIXJORDAN` with `U0FIXJORDAN`), which `conversations.open` opens; Slack lists the app
//! as having Jordan's and Slackbot's. Jordan's holds what they wrote while Dispatch wasn't
//! listening: a message, a photo, a question with a reply in its thread, and the app's own
//! answer, each stamped when a workspace's is first read, so it's newer than when Communications
//! came on; and one from before 2026, before it ever was. A test reads a workspace's once.
use dispatch_core::{Result, db::now};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

/// Jordan's direct message with the app.
pub const JORDAN: &str = "D0FIXJORDAN";
/// The photo Jordan sent, as Slack's own fixture holds it.
const PHOTO: &str = "F0FIXDMPHOTO";

/// Slack's answer to `method` with `args` in the workspace `team`, for a method only
/// Communications calls.
pub fn answer(team: &str, method: &str, args: &Value) -> Option<Result<Value>> {
    let channel = args["channel"].as_str().unwrap_or_default();
    let oldest = args["oldest"].as_str().and_then(seconds).unwrap_or(0.0);
    Some(Ok(match method {
        "conversations.open" => {
            let user = args["users"].as_str().unwrap_or_default();
            json!({"ok": true, "channel": {"id": format!("D{}", user.get(1..).unwrap_or_default())}})
        }
        "users.conversations" => json!({
            "ok": true,
            "channels": [
                {"id": JORDAN, "is_im": true, "user": "U0FIXJORDAN"},
                {"id": "D0FIXSLACKBOT", "is_im": true, "user": "USLACKBOT"},
            ],
            "response_metadata": {"next_cursor": ""},
        }),
        "conversations.history" => {
            let written = if channel == JORDAN { history(team) } else { Vec::new() };
            json!({"ok": true, "messages": since(written, oldest), "has_more": false})
        }
        "conversations.replies" => {
            let thread = args["ts"].as_str().unwrap_or_default();
            let replied: Vec<Value> = if channel == JORDAN {
                let all = history(team).into_iter().chain(replies(team));
                all.filter(|message| message["thread_ts"] == thread).collect()
            } else {
                Vec::new()
            };
            json!({"ok": true, "messages": since(replied, oldest), "has_more": false})
        }
        _ => return None,
    }))
}

/// When the workspace's history was first read, in seconds.
fn stamp(team: &str) -> i64 {
    static STAMPS: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);
    let mut stamps = STAMPS.lock().unwrap_or_else(|poison| poison.into_inner());
    *stamps.entry(team.to_owned()).or_insert_with(|| now() / 1000 + 1)
}
/// Jordan's messages in the workspace, the latest first, as Slack lists a conversation's.
fn history(team: &str) -> Vec<Value> {
    let at = |micros: u32| format!("{}.{micros:06}", stamp(team));
    let thread = at(300_000);
    vec![
        json!({"type": "message", "user": "U0FIXTUREBOT", "bot_id": "B0FIXTURE",
            "text": "DSP Messaging is unavailable", "ts": at(350_000)}),
        json!({"type": "message", "user": "U0FIXJORDAN", "text": "Can I swap Saturday?",
            "ts": thread, "thread_ts": thread, "reply_count": 1, "latest_reply": at(400_000)}),
        json!({"type": "message", "subtype": "file_share", "user": "U0FIXJORDAN", "text": "",
            "ts": at(200_000), "files": [{"id": PHOTO, "name": "Flat tire.png",
            "mimetype": "image/png", "size": 69, "mode": "hosted"}]}),
        json!({"type": "message", "user": "U0FIXJORDAN",
            "text": "Running 10 minutes late, flat tire", "ts": at(100_000)}),
        json!({"type": "message", "user": "U0FIXJORDAN", "text": "Before Communications",
            "ts": "1767225599.000100"}),
    ]
}
/// The replies in the thread of Jordan's question.
fn replies(team: &str) -> Vec<Value> {
    let at = |micros: u32| format!("{}.{micros:06}", stamp(team));
    vec![json!({"type": "message", "user": "U0FIXJORDAN", "text": "Never mind, Tyler took it",
        "ts": at(400_000), "thread_ts": at(300_000)})]
}
/// `messages` written after `oldest`, in seconds, as Slack lists them.
fn since(messages: Vec<Value>, oldest: f64) -> Vec<Value> {
    let written = |message: &Value| message["ts"].as_str().and_then(seconds).unwrap_or(0.0);
    messages
        .into_iter()
        .filter(|message| written(message) > oldest)
        .collect()
}
fn seconds(ts: &str) -> Option<f64> {
    ts.parse().ok()
}
