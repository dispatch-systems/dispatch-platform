//! Queuing a text for a phone, and the pictures added for one: the demo's stand-in delivers what
//! Dispatch sends at once; a picture is its adder's alone until it goes, its bytes kept once
//! however many pictures keep them; and a text or a picture past what one carries is refused
//! before any of it is kept.
use super::*;
use crate::backend::{
    phone::{
        LIMITS, files,
        model::{Added, Status},
        phones,
        support::{kept, store},
    },
    storage::DOMAIN,
};
use axum::body::Bytes;
use dispatch_core::{State, db::now, server::http::upload::Upload, testing};
use futures_util::stream;
use std::sync::Arc;

/// A text `by` sends to `to`, from whichever phone it goes from.
fn to(to: &str, body: String, files: Vec<String>, by: &str) -> Outgoing {
    Outgoing {
        phone: None,
        to: to.into(),
        body,
        files,
        by: by.into(),
    }
}

#[tokio::test]
async fn a_text_sent_from_the_demos_stand_in_phone_is_delivered_at_once() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    crate::backend::phone::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let paired = phones(&db, &dsp).unwrap();
    assert_eq!(paired.len(), 1);
    assert!(paired[0].online);
    let received = db.phones_changes(&dsp, 0).unwrap();
    assert_eq!(received.texts.len(), 3);
    let sent = send(
        &db,
        &dsp,
        &to(
            "(425) 555-0199",
            "Come to the station first".into(),
            Vec::new(),
            &owner,
        ),
        now(),
    )
    .unwrap();
    assert_eq!(sent.phone, paired[0].id);
    assert_eq!(sent.number, "+14255550199");
    assert_eq!(
        sent.status,
        Some(Status::Delivered),
        "the demo's stand-in delivers at once"
    );
    let changed = db.phones_changes(&dsp, received.next).unwrap();
    assert_eq!(changed.texts, [sent]);
}

/// The picture `bytes`, as `by` adds it for a text.
async fn add(state: &Arc<State>, dsp: &str, by: &str, bytes: &'static [u8]) -> String {
    let length = bytes.len() as u64;
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(bytes))]);
    let upload = Upload::streamed(length, LIMITS.largest, chunks)
        .await
        .unwrap();
    let arrived = files::receive(state, dsp, upload).await.unwrap();
    let added = Added {
        by: by.into(),
        name: "door.png".into(),
        mime: "image/png".into(),
    };
    let at = dsp.to_owned();
    state
        .run_scoped(dsp, DOMAIN, move |db| {
            files::keep_added(db, &at, added, &arrived)
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn a_picture_is_its_adders_alone_until_it_goes_and_its_bytes_are_kept_once() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    crate::backend::phone::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let folder = kept(&db, &dsp);
    let other = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let sent = add(&state, &dsp, &owner, b"door").await;
    // Two pictures of the same bytes, which are kept once.
    let unsent = add(&state, &dsp, &owner, b"dock").await;
    let twin = add(&state, &dsp, &owner, b"dock").await;
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
    let outgoing = to(
        "(425) 555-0199",
        "The side door".into(),
        vec![sent.clone()],
        &owner,
    );
    let at = dsp.clone();
    state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            send(db, &at, &outgoing, now())
        })
        .await
        .unwrap();
    let shown = |dsp: &str, id: &str, viewer: &str| {
        let (state, dsp, id, viewer) = (
            state.clone(),
            dsp.to_owned(),
            id.to_owned(),
            viewer.to_owned(),
        );
        async move {
            files::shown(&state, &dsp, &id, &viewer)
                .await
                .unwrap()
                .is_some()
        }
    };

    // Not sent, a picture is its adder's alone; sent, anyone at the DSP sees it. Through another
    // DSP, nobody sees either.
    assert!(shown(&dsp, &unsent, &owner).await && shown(&dsp, &twin, &owner).await);
    assert!(!shown(&dsp, &unsent, "usr_other").await);
    assert!(shown(&dsp, &sent, "usr_other").await);
    assert!(!shown(&other, &sent, &owner).await && !shown(&other, &unsent, &owner).await);
}

#[tokio::test]
async fn a_text_or_a_picture_past_what_one_carries_is_refused_before_any_of_it_is_kept() {
    let (_root, db, dsp) = store();
    let owner = testing::a_user(&db);
    crate::backend::phone::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let folder = kept(&db, &dsp);
    // A text with nothing in it, too long, or with too many pictures is never queued.
    let many = (0..=LIMITS.files).map(|n| format!("file_{n}")).collect();
    let before = db.phones_changes(&dsp, 0).unwrap().next;
    for (outgoing, why) in [
        (
            to("(425) 555-0199", " ".into(), Vec::new(), &owner),
            "nothing in it",
        ),
        (
            to(
                "(425) 555-0199",
                "a".repeat(LIMITS.body + 1),
                Vec::new(),
                &owner,
            ),
            "too long",
        ),
        (
            to("(425) 555-0199", "The side door".into(), many, &owner),
            "too many pictures",
        ),
    ] {
        let refused = send(&db, &dsp, &outgoing, now());
        assert_eq!(refused.unwrap_err().code.as_str(), "invalid_input", "{why}");
    }
    assert_eq!(db.phones_changes(&dsp, before).unwrap().texts, []);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    // A picture past its limit is refused before any of it is kept, however large its upload
    // allowed.
    let length = LIMITS.largest + 1;
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from(vec![
        0;
        length as usize
    ]))]);
    let upload = Upload::streamed(length, u64::MAX, chunks).await.unwrap();
    let refused = files::receive(&state, &dsp, upload).await;
    assert_eq!(refused.err().unwrap().code.as_str(), "upload_too_large");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
}
