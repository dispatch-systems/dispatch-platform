//! What members write to the app directly, kept as Slack tells of it: a message, a file shared,
//! a reply in a thread and an edit, but never the app's own posts, another bot's, or anything
//! sent before Communications was on; and nothing deleted in Slack is deleted here.
use super::*;
use crate::backend::{
    files::{self, FileStatus, FileStore},
    storage::ConversationState,
    support,
};
use dispatch_core::db::now;
use dispatch_slack::testing as slack;
use futures_util::TryStreamExt;
use serde_json::json;

const TEAM: &str = "T0FIXTURE2";
const DM: &str = "D0INCOMING";

/// A message Cameron Hayes wrote to the app in `DM` at `at`, in milliseconds, with `fields`.
fn written(at: i64, fields: Value) -> Value {
    let mut event = json!({"type": "message", "channel_type": "im", "channel": DM,
        "user": "U0FIXCAM", "text": "Flat tire on <#C0FIXGENERAL|route> 4 &amp; 5", "ts": ts(at)});
    for (field, value) in fields.as_object().unwrap() {
        event[field] = value.clone();
    }
    event
}

#[tokio::test]
async fn a_members_messages_and_their_edits_are_kept_and_nothing_else() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let start = now();
    let hear = |event: Value| {
        let (state, dsp) = (state.clone(), dsp.clone());
        async move { heard(&state, &dsp, TEAM, &event).await.unwrap() }
    };
    hear(written(start, json!({}))).await;
    // A photo shared with nothing written, and a reply in the thread of the first.
    hear(written(
        start + 1,
        json!({"subtype": "file_share", "text": "",
        "files": [{"id": "F0FIXDMPHOTO", "name": "Flat tire.png", "mimetype": "image/png",
            "size": 69}]}),
    ))
    .await;
    hear(written(
        start + 2,
        json!({"text": "Fixed it", "thread_ts": ts(start)}),
    ))
    .await;
    // The app's own answer, another bot's post, a deletion and Slackbot's word aren't kept.
    let left = [
        json!({"user": "U0FIXTUREBOT", "bot_id": "B0FIXTURE", "text": "Sent"}),
        json!({"bot_id": "B0OTHER", "subtype": "bot_message", "text": "Ping"}),
        json!({"subtype": "message_deleted", "deleted_ts": ts(start)}),
        json!({"user": "USLACKBOT", "text": "Reminder"}),
    ];
    for (n, fields) in left.into_iter().enumerate() {
        hear(written(start + 10 + n as i64, fields)).await;
    }
    let kept = support::conversation_with(&state, &dsp, TEAM, "U0FIXCAM")
        .await
        .unwrap();
    assert_eq!(kept.name, "Cameron Hayes");
    assert_eq!(kept.channel, DM);
    assert_eq!(kept.state, ConversationState::Waiting);
    assert_eq!(kept.waiting_since, Some(start));
    let messages = support::messages(&state, &dsp, &kept.id).await;
    let read: Vec<(&str, bool)> = messages
        .iter()
        .map(|message| (message.text.as_str(), message.in_thread))
        .collect();
    assert_eq!(
        read,
        [
            ("Flat tire on #route 4 & 5", false),
            ("", false),
            ("Fixed it", true)
        ]
    );
    // The photo is downloaded into Dispatch at once, as Slack keeps it.
    let photo = &support::messages(&state, &dsp, &kept.id).await[1].files[0];
    assert_eq!(
        (photo.name.as_str(), photo.status),
        ("Flat tire.png", FileStatus::Kept)
    );
    let (at, id) = (dsp.clone(), photo.id.clone());
    let sha256 = state
        .read(move |db| Ok(db.communications_file(&at, &id)?.unwrap().sha256.unwrap()))
        .await
        .unwrap();
    let bytes: Vec<u8> = files::read(&state, &dsp, &sha256)
        .await
        .unwrap()
        .map_ok(|chunk| chunk.to_vec())
        .try_concat()
        .await
        .unwrap();
    assert!(bytes.starts_with(b"\x89PNG"));

    // An edit replaces the text, and nothing of what it said before is kept, as Slack sends it.
    let edit = json!({"type": "message", "subtype": "message_changed", "channel": DM,
        "channel_type": "im",
        "message": {"user": "U0FIXCAM", "text": "Flat tire on route 6", "ts": ts(start),
            "edited": {"user": "U0FIXCAM", "ts": ts(start + 60_000)}},
        "previous_message": {"user": "U0FIXCAM", "text": "Flat tire on route 4 & 5",
            "ts": ts(start)}});
    hear(edit).await;
    // Slack adding a link's preview changes no text of its own.
    let preview = json!({"type": "message", "subtype": "message_changed", "channel": DM,
        "message": {"user": "U0FIXCAM", "text": "Unfurled", "ts": ts(start + 2)}});
    hear(preview).await;
    let edited = &support::messages(&state, &dsp, &kept.id).await;
    assert_eq!(
        (edited[0].text.as_str(), edited[0].edited_at),
        ("Flat tire on route 6", Some(start + 60_000))
    );
    assert_eq!(edited[2].text, "Fixed it");
    let at = dsp.clone();
    let stored = state
        .read(move |db| {
            db.dsp(&at)?
                .all("SELECT * FROM communications_messages", [])
        })
        .await
        .unwrap();
    assert!(!serde_json::to_string(&stored).unwrap().contains("4 & 5"));
}

#[tokio::test]
async fn nothing_sent_before_communications_was_on_in_the_workspace_is_kept() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let hear = |event: Value| {
        let (state, dsp) = (state.clone(), dsp.clone());
        async move { heard(&state, &dsp, TEAM, &event).await.unwrap() }
    };
    let kept = || support::conversation_with(&state, &dsp, TEAM, "U0FIXCAM");
    // Sent before the DSP had Communications, as Slack may tell of one late.
    hear(written(now() - 60 * 60 * 1000, json!({}))).await;
    assert!(kept().await.is_none());
    // Sent while it was switched off in the workspace.
    slack::switch(&state, &dsp, TEAM, ID, false).await;
    let between = now() - 5;
    slack::switch(&state, &dsp, TEAM, ID, true).await;
    hear(written(between, json!({}))).await;
    assert!(kept().await.is_none());
    // Sent since, it's kept.
    hear(written(now(), json!({}))).await;
    assert!(kept().await.is_some());
}
