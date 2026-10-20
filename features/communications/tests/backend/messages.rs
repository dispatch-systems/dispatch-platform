//! A workspace forgotten, as on disconnecting it, keeps every message the DSP had there.
use super::*;
use crate::backend::{conversations, storage::CommunicationsStore, support};
use dispatch_core::db::now;
use dispatch_slack::testing as slack;
use serde_json::json;

const TEAM: &str = "T0FIXTURE3";

#[tokio::test]
async fn a_workspace_forgotten_keeps_its_messages_and_lets_go_of_conversations_without_any() {
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    let event = json!({"type": "message", "channel_type": "im", "channel": "D0FORGET",
        "user": "U0FIXOWNER", "text": "Keys are in the van", "ts": incoming::ts(now())});
    incoming::heard(&state, &dsp, TEAM, &event).await.unwrap();
    let at = dsp.clone();
    state
        .run(move |db| {
            db.open_communications_conversation(&at, TEAM, "U0NOWRITE", "D0NOWRITE", "Quiet")
                .map(drop)
        })
        .await
        .unwrap();
    slack::disconnect(&state, &dsp, TEAM).await;
    let kept = support::conversation_with(&state, &dsp, TEAM, "U0FIXOWNER")
        .await
        .unwrap();
    assert_eq!(kept.name, "Morgan Lane");
    assert_eq!(
        support::texts(&state, &dsp, &kept.id).await,
        ["Keys are in the van"]
    );
    // Its workspace gone, it can't be written in.
    let (shared, at, id) = (state.clone(), dsp.clone(), kept.id.clone());
    let listed = state
        .read(move |db| conversations::one(db, &shared, &at, &id))
        .await
        .unwrap();
    assert!(!listed.can_reply);
    assert!(
        support::conversation_with(&state, &dsp, TEAM, "U0NOWRITE")
            .await
            .is_none()
    );
}
