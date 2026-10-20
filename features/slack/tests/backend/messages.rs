//! What Slack answers a message written to the app that no integration took: only one a person
//! wrote anew is answered, so the app's own answer, or another bot's, is never answered in turn.
use super::*;
use crate::backend::{app, fixture, support};

const TEAM: &str = "T0FIXTURE1";
const DM: &str = "D0FIXMESSAGES";

/// `written` with `changed` in place of what it has, or beside it.
fn with(written: &Value, changed: Value) -> Value {
    let mut event = written.clone();
    for (field, value) in changed.as_object().unwrap() {
        event[field] = value.clone();
    }
    event
}

#[tokio::test]
async fn only_a_message_a_person_wrote_anew_is_answered_that_dsp_messaging_is_unavailable() {
    let (_root, state, dsp) = support::platform();
    let bot = support::connect(&state, &dsp, TEAM, &app::scopes()).await;
    let written = json!({"type": "message", "channel_type": "im", "channel": DM,
        "user": "U0FIXJORDAN", "text": "Running late", "ts": "1760000000.000100"});
    let left = [
        // The app's own answer, and another bot's post.
        with(
            &written,
            json!({"user": "U0FIXTUREBOT", "bot_id": "B0FIXTURE"}),
        ),
        with(&written, json!({"bot_id": "B0OTHER"})),
        // An edit, a deletion and a file shared.
        with(&written, json!({"subtype": "message_changed"})),
        with(&written, json!({"subtype": "message_deleted"})),
        with(&written, json!({"subtype": "file_share"})),
    ];
    for event in &left {
        unheard(&bot, event).await.unwrap();
    }
    assert!(fixture::posted(TEAM, DM).is_empty());
    unheard(&bot, &written).await.unwrap();
    assert_eq!(fixture::posted(TEAM, DM), ["DSP Messaging is unavailable"]);
}
