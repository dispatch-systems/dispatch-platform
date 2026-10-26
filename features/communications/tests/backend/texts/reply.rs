//! Answering by text: what's written goes as written, from the DSP's phone, under whoever wrote
//! it on the page; one that fails leaves the conversation waiting, and Retry sends it again.
use super::*;
use crate::ConversationState;
use crate::backend::{
    support::{self, texted},
    texts::{
        reading::one,
        storage::{TextStatus, TextStore},
    },
};
use dispatch_core::{db::now, testing};

#[tokio::test]
async fn an_answer_that_failed_leaves_it_waiting_and_retry_sends_it_again() {
    support::install();
    let (_root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let asked = texted(
        &state,
        &dsp,
        "+12535550177",
        "Can I swap Saturday?",
        now() - 60_000,
    )
    .await;
    let (at, id) = (dsp.clone(), asked.clone());
    let conversation = state
        .read(move |db| Ok(db.communications_text(&at, &id)?.unwrap().conversation))
        .await
        .unwrap();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let state_of = || {
        let (shared, at, id) = (state.clone(), dsp.clone(), conversation.clone());
        state.read(move |db| Ok(one(db, &shared, &at, &id)?.state))
    };

    // The phone fails to send it: the answer shows so, under who wrote it, and it still waits.
    support::failing(&dsp, true);
    let failed = send(
        &state,
        &c,
        support::USE,
        &conversation,
        "Perfect, thank you!",
    )
    .await
    .unwrap();
    let sent = support::sent(&dsp);
    assert_eq!(
        (
            sent[0].to.as_str(),
            sent[0].body.as_str(),
            sent[0].by.as_str()
        ),
        (
            "+12535550177",
            "Perfect, thank you!",
            c.auth.user.id.as_str()
        )
    );
    assert_eq!(
        (failed.status, failed.sender.unwrap().name.as_str()),
        (Some(TextStatus::Failed), "Test Owner")
    );
    assert_eq!(state_of().await.unwrap(), ConversationState::Waiting);

    // Sent again, it's on its way: an answer.
    support::failing(&dsp, false);
    let again = retry(&state, &c, support::USE, &conversation, &failed.id)
        .await
        .unwrap();
    assert_eq!(
        (again.id, again.status),
        (failed.id, Some(TextStatus::Waiting))
    );
    assert_eq!(state_of().await.unwrap(), ConversationState::Responded);
    // Another conversation's text isn't this one's to send again.
    let elsewhere = texted(&state, &dsp, "+14255550199", "Hi", now()).await;
    let refused = retry(&state, &c, support::USE, &conversation, &elsewhere).await;
    assert_eq!(
        refused.unwrap_err().code,
        "communications_message_not_found"
    );
}
