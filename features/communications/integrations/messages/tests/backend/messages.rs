//! A workspace forgotten, as on disconnecting it, keeps every message the DSP had there; and
//! which of the DSP's workspaces need updating for Communications.
use super::*;
use crate::backend::{conversations, reply, storage::CommunicationsStore, support};
use dispatch_core::db::now;
use dispatch_slack::testing as slack;
use serde_json::json;

const TEAM: &str = "T0FIXTURE3";

/// The texts of the conversation's messages, in the order written.
async fn texts(state: &Arc<State>, dsp: &str, conversation: &str) -> Vec<String> {
    support::messages(state, dsp, conversation)
        .await
        .into_iter()
        .map(|message| message.text)
        .collect()
}

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
    assert_eq!(texts(&state, &dsp, &kept.id).await, ["Keys are in the van"]);
    // Its workspace gone, it can't be written in.
    let owner = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let refused = reply::send(&state, &owner, support::USE, &kept.id, "Still there?", &[])
        .await
        .unwrap_err();
    assert_eq!(refused.code, "communications_off");
    assert!(
        support::conversation_with(&state, &dsp, TEAM, "U0NOWRITE")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn a_workspace_needs_updating_only_while_communications_is_on_there_without_what_it_uses() {
    // The admin of the first approved what Communications uses, if not all Slack asks for.
    let (_root, state, bot) = support::connected(TEAM).await;
    let dsp = bot.dsp.clone();
    // The second's approved the app before it answered direct messages.
    let earlier = &["users:read", "users:read.email", "im:history"];
    slack::connect(&state, &dsp, "T0FIXTURE2", earlier).await;
    let needs = || {
        let at = dsp.clone();
        state.read(move |db| {
            let listed = conversations::workspaces(db, &at)?;
            Ok(listed
                .into_iter()
                .map(|workspace| (workspace.team_id, workspace.needs_update))
                .collect::<Vec<_>>())
        })
    };
    let named = |second: bool| vec![("T0FIXTURE2".to_owned(), second), (TEAM.to_owned(), false)];
    assert_eq!(needs().await.unwrap(), named(true));
    // Switched off there, it needs nothing there.
    slack::switch(&state, &dsp, "T0FIXTURE2", ID, false).await;
    assert_eq!(needs().await.unwrap(), named(false));
}
