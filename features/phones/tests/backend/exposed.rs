//! What Phones exposes, as a feature using it finds it: only while it's on for the DSP, with
//! the DSP's phones and the texts they relay, sending one from them, sending one that failed again
//! as its caller's condition allows, and the pictures added for one, let go of while not sent.
use crate::backend::{
    storage::{StatusReport, TextStore},
    support::{always, kept},
};
use axum::body::Bytes;
use dispatch_core::{Error, State, db::Store, ensure, server::http::upload::Upload, testing};
use dispatch_phones_contract::{Added, Outgoing, Status, Texting, for_dsp};
use futures_util::stream;
use std::sync::Arc;

#[tokio::test]
async fn texting_sends_from_the_dsps_phone_while_phones_is_on() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    crate::backend::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let texting = for_dsp(&db, &dsp).unwrap().expect("Phones is on");
    let phones = texting.phones(&db, &dsp).unwrap();
    assert_eq!(phones.len(), 1);
    assert!(phones[0].online);
    let received = texting.changes(&db, &dsp, 0).unwrap();
    assert_eq!(received.texts.len(), 3);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let outgoing = Outgoing {
        phone: None,
        to: "(425) 555-0199".into(),
        body: "Come to the station first".into(),
        files: Vec::new(),
        by: owner.clone(),
    };
    let sent = texting
        .send(&state, &dsp, outgoing, always())
        .await
        .unwrap();
    assert_eq!(sent.phone, phones[0].id);
    assert_eq!(sent.number, "+14255550199");
    assert_eq!(
        sent.status,
        Some(Status::Delivered),
        "the demo's stand-in delivers at once"
    );
    let (at, after) = (dsp.clone(), received.next);
    let changed = state
        .read(move |db| texting.changes(db, &at, after))
        .await
        .unwrap();
    assert_eq!(changed.texts, [sent]);

    let at = dsp.clone();
    state
        .run(move |db| db.set_feature(&at, "phones", false, &owner))
        .await
        .unwrap();
    let at = dsp.clone();
    let offered = state.read(move |db| for_dsp(db, &at)).await.unwrap();
    assert!(offered.is_none());
}

#[tokio::test]
async fn texting_sends_a_text_or_a_failed_one_again_only_as_its_condition_allows_in_the_write_queuing_it()
 {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    crate::backend::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let texting = for_dsp(&db, &dsp).unwrap().expect("Phones is on");
    // A text Dispatch sent, which the phone says failed.
    let outgoing = Outgoing {
        phone: None,
        to: "(425) 555-0199".into(),
        body: "Van 214".into(),
        files: Vec::new(),
        by: owner.clone(),
    };
    let failed = db.phones_send(&dsp, &outgoing, 1_000).unwrap();
    let (_, attempt) = db
        .phones_outbox(&dsp, &failed.phone)
        .unwrap()
        .into_iter()
        .find(|(text, _)| text.id == failed.id)
        .expect("it waits for the phone");
    let report = StatusReport {
        id: failed.id.clone(),
        attempt,
        status: Status::Failed,
        at: 2_000,
        error: Some("No service".into()),
    };
    assert!(db.phones_status(&dsp, &failed.phone, &report).unwrap());
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();

    // A text refused by its condition is neither kept nor queued.
    let refused = Box::new(|_: &Store| Err(Error::new("condition_refused", 409)));
    let unsent = Outgoing {
        phone: None,
        to: "(425) 555-0199".into(),
        body: "Gate 4411".into(),
        files: Vec::new(),
        by: owner,
    };
    let answer = texting.send(&state, &dsp, unsent, refused).await;
    assert_eq!(answer.unwrap_err().code.as_str(), "condition_refused");
    let (at, phone) = (dsp.clone(), failed.phone.clone());
    let queued = state.read(move |db| db.phones_outbox(&at, &phone)).await;
    assert!(queued.unwrap().is_empty());

    // Refused by its condition, a failed one stays failed.
    let refused = Box::new(|_: &Store| Err(Error::new("condition_refused", 409)));
    let answer = texting.retry(&state, &dsp, &failed.id, refused).await;
    assert_eq!(answer.unwrap_err().code.as_str(), "condition_refused");
    let (at, id) = (dsp.clone(), failed.id.clone());
    let kept = state.read(move |db| db.phones_text(&at, &id)).await;
    assert_eq!(kept.unwrap().unwrap().status, Some(Status::Failed));
    // Allowed, it waits on the phone again; its condition held the platform's lock alone, so no
    // other write came between it and the text queued.
    let shared = state.clone();
    let alone = Box::new(move |_: &Store| {
        ensure(
            shared.transition.try_read().is_err(),
            "condition_shared",
            500,
        )
    });
    let retried = texting
        .retry(&state, &dsp, &failed.id, alone)
        .await
        .unwrap();
    assert_eq!(retried.status, Some(Status::Waiting));
}

/// The picture `bytes`, as `by` adds it for a text.
async fn add(
    texting: &dyn Texting,
    state: &Arc<State>,
    dsp: &str,
    by: &str,
    bytes: &'static [u8],
) -> String {
    let length = bytes.len() as u64;
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(bytes))]);
    let upload = Upload::streamed(length, texting.largest(), chunks)
        .await
        .unwrap();
    let added = Added {
        by: by.into(),
        name: "door.png".into(),
        mime: "image/png".into(),
    };
    texting
        .add_file(state, dsp, added, upload)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn texting_lets_go_of_a_picture_only_its_adder_has_not_sent() {
    testing::install(&[], &[&crate::FEATURE]);
    let (_root, db, dsp) = testing::bootstrapped();
    let owner = testing::a_user(&db);
    crate::backend::demo::demo(&db, &dsp, "America/Los_Angeles").unwrap();
    let texting = for_dsp(&db, &dsp).unwrap().expect("Phones is on");
    let folder = kept(&db, &dsp);
    let other = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let sent = add(texting, &state, &dsp, &owner, b"door").await;
    // Two pictures of the same bytes, which are kept once.
    let unsent = add(texting, &state, &dsp, &owner, b"dock").await;
    let twin = add(texting, &state, &dsp, &owner, b"dock").await;
    let bytes_kept = || std::fs::read_dir(&folder).unwrap().count();
    assert_eq!(bytes_kept(), 2);
    let outgoing = Outgoing {
        phone: None,
        to: "(425) 555-0199".into(),
        body: "The side door".into(),
        files: vec![sent.clone()],
        by: owner.clone(),
    };
    texting
        .send(&state, &dsp, outgoing, always())
        .await
        .unwrap();
    let shown = |id: String| {
        let (state, dsp, owner) = (state.clone(), dsp.clone(), owner.clone());
        async move {
            texting
                .file(&state, &dsp, &id, &owner)
                .await
                .unwrap()
                .is_some()
        }
    };

    // Another's to let go of, sent, or named through another DSP, it stays.
    texting
        .remove_file(&state, &dsp, &unsent, "usr_other")
        .await
        .unwrap();
    texting
        .remove_file(&state, &dsp, &sent, &owner)
        .await
        .unwrap();
    texting
        .remove_file(&state, &other, &unsent, &owner)
        .await
        .unwrap();
    assert!(shown(unsent.clone()).await && shown(sent.clone()).await);
    assert_eq!(bytes_kept(), 2);
    // Its adder's and not sent, it goes, its bytes kept while its twin keeps them, and gone with
    // the last; the sent one's stay.
    texting
        .remove_file(&state, &dsp, &unsent, &owner)
        .await
        .unwrap();
    assert!(!shown(unsent).await);
    assert!(shown(twin.clone()).await);
    assert_eq!(bytes_kept(), 2);
    texting
        .remove_file(&state, &dsp, &twin, &owner)
        .await
        .unwrap();
    assert!(!shown(twin).await);
    assert!(shown(sent).await);
    assert_eq!(bytes_kept(), 1);
}
