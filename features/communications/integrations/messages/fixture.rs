//! Fixture mode's answers to the Slack methods only Communications calls, so previews and tests
//! never reach Slack. The app has a direct message with each member, `D` and the rest of their
//! ID (`D0FIXJORDAN` with `U0FIXJORDAN`), which `conversations.open` opens, holding what the app
//! posted there; Slack lists the app as having Jordan's and Slackbot's. Jordan's holds what they wrote while Dispatch wasn't
//! listening: a message, a photo, a question with a reply in its thread, and the app's own
//! answer, each stamped when a workspace's is first read, so it's newer than when Communications
//! came on; and one from before 2026, before it ever was. A test reads a workspace's once, and
//! may add a reply to the question later (`reply_later`). In Managers, Slack lists the app as
//! having Cameron's too, holding more than a pass reads. Each lists as Slack does: the latest
//! first, within `oldest` and `latest`, a page at a time.
use dispatch_core::{Result, db::now};
use dispatch_slack::kit;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

/// Jordan's direct message with the app.
pub const JORDAN: &str = "D0FIXJORDAN";
/// Cameron's, in the one workspace where it holds more than a pass reads, and how many.
pub const BUSY: &str = "D0FIXCAM";
pub const BUSY_TEAM: &str = "T0FIXTURE3";
const CHATTER: usize = 2050;
/// The photo Jordan sent, as Slack's own fixture holds it.
const PHOTO: &str = "F0FIXDMPHOTO";

/// Slack's answer to `method` with `args` in the workspace `team`, for a method only
/// Communications calls.
pub fn answer(team: &str, method: &str, args: &Value) -> Option<Result<Value>> {
    let channel = args["channel"].as_str().unwrap_or_default();
    let busy = team == BUSY_TEAM;
    Some(Ok(match method {
        "conversations.open" => {
            let user = args["users"].as_str().unwrap_or_default();
            json!({"ok": true, "channel": {"id": format!("D{}", user.get(1..).unwrap_or_default())}})
        }
        "users.conversations" => {
            let mut channels = vec![
                json!({"id": JORDAN, "is_im": true, "user": "U0FIXJORDAN"}),
                json!({"id": "D0FIXSLACKBOT", "is_im": true, "user": "USLACKBOT"}),
            ];
            if busy {
                channels.push(json!({"id": BUSY, "is_im": true, "user": "U0FIXCAM"}));
            }
            json!({"ok": true, "channels": channels, "response_metadata": {"next_cursor": ""}})
        }
        "conversations.history" => {
            let mut messages = match channel {
                JORDAN => history(team),
                BUSY if busy => chatter(team),
                _ => Vec::new(),
            };
            // What the app posted there too, as Slack lists it among them.
            messages.extend(kit::fixture_history(team, channel));
            messages.sort_by(|a, b| b["ts"].as_str().cmp(&a["ts"].as_str()));
            listed(messages, args)
        }
        "conversations.replies" => {
            let thread = args["ts"].as_str().unwrap_or_default();
            let replied: Vec<Value> = if channel == JORDAN {
                let all = history(team).into_iter().chain(replies(team));
                all.filter(|message| message["thread_ts"] == thread)
                    .collect()
            } else {
                Vec::new()
            };
            listed(replied, args)
        }
        _ => return None,
    }))
}

/// When the workspace's history was first read, in seconds.
fn stamp(team: &str) -> i64 {
    static STAMPS: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);
    let mut stamps = STAMPS.lock().unwrap_or_else(|poison| poison.into_inner());
    *stamps
        .entry(team.to_owned())
        .or_insert_with(|| now() / 1000 + 1)
}
/// Jordan's messages in the workspace, the latest first, as Slack lists a conversation's.
fn history(team: &str) -> Vec<Value> {
    let at = |micros: u32| format!("{}.{micros:06}", stamp(team));
    let thread = at(300_000);
    let replied = replies(team);
    vec![
        json!({"type": "message", "user": "U0FIXTUREBOT", "bot_id": "B0FIXTURE",
            "text": "DSP Messaging is unavailable", "ts": at(350_000)}),
        json!({"type": "message", "user": "U0FIXJORDAN", "text": "Can I swap Saturday?",
            "ts": thread, "thread_ts": thread, "reply_count": replied.len(),
            "latest_reply": replied.last().map(|reply| reply["ts"].clone())}),
        json!({"type": "message", "subtype": "file_share", "user": "U0FIXJORDAN", "text": "",
            "ts": at(200_000), "files": [{"id": PHOTO, "name": "Flat tire.png",
            "mimetype": "image/png", "size": 69, "mode": "hosted"}]}),
        json!({"type": "message", "user": "U0FIXJORDAN",
            "text": "Running 10 minutes late, flat tire", "ts": at(100_000)}),
        json!({"type": "message", "user": "U0FIXJORDAN", "text": "Before Communications",
            "ts": "1767225599.000100"}),
    ]
}
/// The replies in the thread of Jordan's question: the one they wrote, then any a test added.
fn replies(team: &str) -> Vec<Value> {
    let at = |micros: u32| format!("{}.{micros:06}", stamp(team));
    let written = json!({"type": "message", "user": "U0FIXJORDAN",
        "text": "Never mind, Tyler took it", "ts": at(400_000), "thread_ts": at(300_000)});
    let added = ADDED.lock().unwrap_or_else(|poison| poison.into_inner());
    let later = added
        .iter()
        .filter(|(of, ..)| of == team)
        .map(|(_, text, ts)| {
            json!({"type": "message", "user": "U0FIXJORDAN", "text": text, "ts": ts,
            "thread_ts": at(300_000)})
        });
    [written].into_iter().chain(later).collect()
}
/// The replies a test added to Jordan's question later: each one's workspace, text and `ts`.
static ADDED: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());
/// Adds Jordan's reply `text` to their question in the workspace, written `after` seconds after
/// its history was stamped.
#[cfg(test)]
pub fn reply_later(team: &str, text: &str, after: i64) {
    let ts = format!("{}.000000", stamp(team) + after);
    let mut added = ADDED.lock().unwrap_or_else(|poison| poison.into_inner());
    added.push((team.to_owned(), text.to_owned(), ts));
}
/// Cameron's messages in the workspace with more of them than a pass reads, the latest first,
/// each a microsecond after the last from when its history was stamped.
fn chatter(team: &str) -> Vec<Value> {
    (1..=CHATTER)
        .rev()
        .map(|n| {
            json!({"type": "message", "user": "U0FIXCAM", "text": format!("Stop {n} done"),
                "ts": format!("{}.{n:06}", stamp(team))})
        })
        .collect()
}
/// `messages`, the latest first, as Slack lists them with `args`: those after `oldest` and before
/// `latest`, or at either too when `inclusive`, `limit` at a time from `cursor`. Slack's `ts` are
/// all of one width, so they compare as text.
fn listed(messages: Vec<Value>, args: &Value) -> Value {
    let bound = |field: &str| args[field].as_str().unwrap_or_default().to_owned();
    let (oldest, latest, inclusive) = (bound("oldest"), bound("latest"), args["inclusive"] == true);
    let within: Vec<Value> = messages
        .into_iter()
        .filter(|message| {
            let ts = message["ts"].as_str().unwrap_or_default();
            let after = if inclusive {
                ts >= oldest.as_str()
            } else {
                ts > oldest.as_str()
            };
            let before = latest.is_empty()
                || if inclusive {
                    ts <= latest.as_str()
                } else {
                    ts < latest.as_str()
                };
            after && before
        })
        .collect();
    let limit = args["limit"].as_u64().unwrap_or(100) as usize;
    let from: usize = args["cursor"]
        .as_str()
        .and_then(|at| at.parse().ok())
        .unwrap_or(0);
    let next = if from + limit < within.len() {
        (from + limit).to_string()
    } else {
        String::new()
    };
    let page: Vec<Value> = within.into_iter().skip(from).take(limit).collect();
    json!({"ok": true, "messages": page, "has_more": !next.is_empty(),
        "response_metadata": {"next_cursor": next}})
}
