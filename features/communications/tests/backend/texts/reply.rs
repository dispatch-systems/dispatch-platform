//! Answering by text: what's written goes as written, from the DSP's phone, under whoever wrote
//! it on the page, as much as a text carries; one that fails leaves the conversation waiting, and
//! Retry sends it again. Neither goes to a work phone that isn't the driver's alone now, even as it
//! changed hands once it was asked for, nor once its writer's access ended, each checked in the
//! write that queues it. A picture added for one keeps nothing once its adder's access ended while
//! its bytes came.
use super::*;
use crate::ConversationState;
use crate::backend::{
    phone::{model::Status, storage::FileStore},
    support::{self, filed, phoned, phones, read_live, reported, saved, switch, texted, today},
    texts::{model::TextStatus, reading, reading::one, storage::TextStore},
};
use axum::body::Bytes;
use dispatch_core::{db::now, server::http::upload::Upload};
use futures_util::{StreamExt, stream};

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
    let answer = send(
        &state,
        &c,
        support::USE,
        &conversation,
        "Perfect, thank you!",
        &[],
    )
    .await
    .unwrap();
    let sent = support::sent(&state, &dsp).await;
    assert_eq!(
        sent,
        [(
            "+12535550177".to_owned(),
            "Perfect, thank you!".to_owned(),
            c.auth.user.id.clone()
        )]
    );
    reported(&state, &dsp, &answer.id, Status::Failed).await;
    let (shared, at, id) = (state.clone(), dsp.clone(), answer.id.clone());
    let failed = state
        .read(move |db| reading::message(db, &shared, &at, &id))
        .await
        .unwrap();
    assert_eq!(
        (failed.status, failed.sender.unwrap().name.as_str()),
        (Some(TextStatus::Failed), "Test Owner")
    );
    assert_eq!(state_of().await.unwrap(), ConversationState::Waiting);

    // Sent again, it's on its way: an answer.
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
async fn retry_refuses_a_drivers_answer_once_the_work_phone_it_went_to_is_not_theirs_as_it_goes() {
    let (_root, state, dsp) = support::started();
    let (_, day) = today(&state, &dsp).await;
    let today = day.to_string();
    // Avery has 0101 and texts; the answer through it fails.
    let avery = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&today), avery, Vec::new()).await;
    let asked = texted(&state, &dsp, "+12065550101", "Which van?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let failed = send(&state, &c, support::USE, &conversation, "Van 214", &[])
        .await
        .unwrap();
    reported(&state, &dsp, &failed.id, Status::Failed).await;
    let retried = || retry(&state, &c, support::USE, &conversation, &failed.id);

    // Once Retry is asked for, a read shows Avery with 0202 and Blake with 0101, before filing
    // hears of it: it would reach Blake.
    let handed = phones(&[("A1", "2065550202"), ("B2", "2065550101")]);
    read_live(&dsp, Some(&today), handed);
    assert_eq!(
        retried().await.unwrap_err().code,
        "communications_work_phone_changed"
    );
    // The next shows 0101 as both Avery's and Blake's latest: it's neither's alone.
    let both = phones(&[("A1", "2065550101"), ("B2", "2065550101")]);
    read_live(&dsp, Some(&today), both);
    assert_eq!(
        retried().await.unwrap_err().code,
        "communications_no_work_phone"
    );
    // Then the stop saves the day with 0101 Avery's again, and live route data empties: Avery has
    // no work phone now.
    let theirs = saved(day, &[("A1", "2065550101")]);
    phoned(&state, &dsp, None, Vec::new(), vec![theirs]).await;
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
async fn an_answer_goes_to_the_work_phone_its_driver_has_as_it_is_queued() {
    let (_root, state, dsp) = support::started();
    let (_, day) = today(&state, &dsp).await;
    let today = day.to_string();
    // Avery has 0101 and texts.
    let avery = phones(&[("A1", "2065550101")]);
    phoned(&state, &dsp, Some(&today), avery, Vec::new()).await;
    let asked = texted(&state, &dsp, "+12065550101", "Which van?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;

    // Once the answer is asked for, a read shows Avery with 0202 and Blake with 0101, before
    // filing hears of it: it reaches Avery, never Blake.
    let handed = phones(&[("A1", "2065550202"), ("B2", "2065550101")]);
    read_live(&dsp, Some(&today), handed);
    send(&state, &c, support::USE, &conversation, "Van 214", &[])
        .await
        .unwrap();
    let went: Vec<String> = support::sent(&state, &dsp)
        .await
        .into_iter()
        .map(|(to, _, _)| to)
        .collect();
    assert_eq!(went, ["+12065550202"], "nothing went to Blake");
}

#[tokio::test]
async fn an_answer_never_goes_once_its_writers_access_ended_as_it_goes() {
    let (_root, state, dsp) = support::started();
    let asked = texted(&state, &dsp, "+12535550177", "Can I swap Saturday?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    // Communications goes off for the DSP once the answer is asked for, and with it their
    // permission: refused, nothing goes.
    switch(&state, &dsp, "communications", false).await;
    let refused = send(&state, &c, support::USE, &conversation, "Swap it", &[]).await;
    assert_eq!(refused.unwrap_err().code, "permission_denied");
    assert!(support::sent(&state, &dsp).await.is_empty(), "nothing went");
}

/// `bytes` as a picture of a PNG, the upload's last chunk coming once `last` is done.
async fn picture(bytes: &'static [u8], last: impl Future<Output = ()> + Send + 'static) -> Upload {
    let first = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(&bytes[..1]))]);
    let rest = stream::once(async move {
        last.await;
        Ok(Bytes::from_static(&bytes[1..]))
    });
    Upload::streamed(bytes.len() as u64, u64::MAX, first.chain(rest))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_picture_keeps_nothing_once_its_adders_access_ended_while_its_bytes_came() {
    let (_root, state, dsp) = support::started();
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let photo = picture(b"door", async {}).await;
    let kept = upload(&state, &c, support::USE, "door.png", "image/png", photo)
        .await
        .unwrap();
    // Communications goes off for the DSP while the next one's bytes come, and with it their
    // permission: refused, the phone side keeps none of it.
    let (shared, at_dsp) = (state.clone(), dsp.clone());
    let off = async move { switch(&shared, &at_dsp, "communications", false).await };
    let photo = picture(b"dock", off).await;
    let refused = upload(&state, &c, support::USE, "dock.png", "image/png", photo).await;
    assert_eq!(refused.unwrap_err().code, "permission_denied");
    let at_dsp = dsp.clone();
    let bytes = state
        .read(move |db| db.phones_kept_bytes(&at_dsp))
        .await
        .unwrap();
    assert_eq!(bytes.len(), 1, "only the first's: {bytes:?}");
    let (at_dsp, id) = (dsp.clone(), kept.id.clone());
    let first = state
        .read(move |db| db.phones_file(&at_dsp, &id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.sha256.as_deref(), Some(bytes[0].as_str()));
}

#[tokio::test]
async fn an_answer_carries_as_much_as_a_text_does_not_what_slacks_does() {
    let (_root, state, dsp) = support::started();
    let asked = texted(&state, &dsp, "+12535550177", "Can I swap Saturday?", now()).await;
    let conversation = filed(&state, &dsp, &asked).await.id;
    let c = support::member(&state, &dsp, "Test", "Owner", "owner@example.test").await;
    let mut pictures = Vec::new();
    for _ in 0..11 {
        let photo = picture(b"png", async {}).await;
        let added = upload(&state, &c, support::USE, "door.png", "image/png", photo).await;
        pictures.push(added.unwrap().id);
    }
    let answer = |written: String, photos: &[String]| {
        let (state, c, conversation, photos) = (&state, &c, &conversation, photos.to_vec());
        async move {
            send(state, c, support::USE, conversation, &written, &photos)
                .await
                .map(|sent| sent.text)
                .map_err(|refused| refused.code)
        }
    };

    // A text carries 10,000 characters, where a Slack message carries 3,000, and ten pictures.
    let most = "a".repeat(10_000);
    let refused = Err("invalid_input".to_owned());
    assert_eq!(answer(format!("{most}a"), &[]).await, refused);
    assert_eq!(answer(String::new(), &pictures).await, refused);
    assert_eq!(answer(most.clone(), &[]).await, Ok(most));
    assert!(answer(String::new(), &pictures[..10]).await.is_ok());
    assert_eq!(
        support::sent(&state, &dsp).await.len(),
        2,
        "only those within a text's limits went"
    );
}
