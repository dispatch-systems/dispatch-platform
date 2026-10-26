//! Answering by text: what's written goes as written, from the DSP's phone, under whoever wrote
//! it on the page; one that fails leaves the conversation waiting, and Retry sends it again, but
//! never to a work phone the driver no longer holds. A picture added for one is let go of once its
//! adder's access ended while it came.
use super::*;
use crate::ConversationState;
use crate::backend::{
    support::{self, at, filed, held, hold, texted, today},
    texts::{
        reading::one,
        storage::{TextStatus, TextStore},
    },
};
use axum::body::Bytes;
use dispatch_core::{db::now, server::http::upload::Upload};
use futures_util::stream;

#[tokio::test]
async fn an_answer_that_failed_leaves_it_waiting_and_retry_sends_it_again() {
    let (_root, state, dsp) = support::started();
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
        &[],
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

#[tokio::test]
async fn retry_refuses_a_drivers_answer_once_the_work_phone_it_went_to_is_not_theirs() {
    let (_root, state, dsp) = support::started();
    let (zone, day) = today(&state, &dsp).await;
    let (midnight, today) = (at(zone, day, 0, 0), day.to_string());
    // Avery holds 0101 and texts; the answer through it fails.
    let avery = vec![hold("A1", "2065550101", midnight, None)];
    held(&state, &dsp, Some(&today), avery, Vec::new()).await;
    let asked = texted(&state, &dsp, "+12065550101", "Which van?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    support::failing(&dsp, true);
    let failed = send(&state, &c, support::USE, &conversation, "Van 214", &[])
        .await
        .unwrap();
    support::failing(&dsp, false);
    let retried = || retry(&state, &c, support::USE, &conversation, &failed.id);

    // Avery hands 0101 to Blake and takes 0202: sent again, it would reach Blake.
    let handoff = failed.at + 1;
    let holds = vec![
        hold("A1", "2065550101", midnight, Some(handoff)),
        hold("A1", "2065550202", handoff, None),
        hold("B2", "2065550101", handoff, None),
    ];
    held(&state, &dsp, Some(&today), holds, Vec::new()).await;
    assert_eq!(
        retried().await.unwrap_err().code,
        "communications_work_phone_changed"
    );
    // At the stop Avery holds none.
    held(&state, &dsp, None, Vec::new(), Vec::new()).await;
    assert_eq!(
        retried().await.unwrap_err().code,
        "communications_no_work_phone"
    );
    let (at_dsp, id) = (dsp.clone(), failed.id.clone());
    let kept = state
        .read(move |db| db.communications_text(&at_dsp, &id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (kept.conversation, kept.status),
        (conversation.clone(), Some(TextStatus::Failed)),
        "still Avery's, and never sent again"
    );
}

#[tokio::test]
async fn a_picture_is_let_go_once_its_adders_access_ended_while_its_bytes_came() {
    let (_root, state, dsp) = support::started();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let add = || async {
        let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"png"))]);
        let photo = Upload::streamed(3, u64::MAX, chunks).await.unwrap();
        upload(&state, &c, support::USE, "door.png", "image/png", photo).await
    };
    let kept = add().await.unwrap();
    // Communications goes off for the DSP as the phone keeps the next one's bytes, and with it
    // their permission: refused, the phone keeps none of it.
    support::off_while_reading(&dsp);
    assert_eq!(add().await.unwrap_err().code, "permission_denied");
    assert_eq!(support::added(&dsp), [kept.id]);
}
