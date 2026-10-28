//! The bytes texts carry, as they arrive: a reported file's are kept with their own size, those
//! sent again leave those before them kept should what keeps them fail, those cut short leave
//! nothing behind, and those gone once upkeep listed them went already. A picture added for a text
//! is its adder's alone until it goes, its bytes kept once however many pictures keep them, and one
//! past what a text carries is refused before any of it is kept.
use super::*;
use crate::backend::{
    phone::{
        outbox::send,
        phones,
        storage::TextStore,
        support::{kept, outgoing, paired, reported, store},
    },
    storage::DOMAIN,
    support::started,
    texts::filing,
};
use axum::body::Bytes;
use dispatch_core::{db::now, testing};
use futures_util::{StreamExt, stream};
use std::task::Poll;

/// `bytes`, as an upload brings them.
async fn upload(bytes: &'static [u8]) -> Upload {
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(bytes))]);
    Upload::streamed(bytes.len() as u64, u64::MAX, chunks)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_reported_files_size_is_what_its_bytes_are() {
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    // The phone said its picture is 100 bytes; four come.
    let kept = db
        .phones_report(
            &dsp,
            &phone.id,
            vec![reported("a", "+14255550199", 5_000, 1)],
        )
        .unwrap();
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let arrived = receive(&state, &dsp, upload(b"door").await).await.unwrap();
    let (at, file) = (dsp.clone(), kept[0].files[0].clone());
    let of = state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            keep_reported(db, &at, &phone.id, &file, &arrived)
        })
        .await
        .unwrap();
    assert_eq!(of, kept[0].id, "the text it's of, for filing");
    let (at, file) = (dsp.clone(), kept[0].files[0].clone());
    let stored = state
        .read(move |db| db.phones_file(&at, &file))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((stored.file.size, stored.file.kept), (4, true));
}

/// The DSP's phone `phone` sends `bytes` as those of the file `id` of a text it reported, as its
/// app does: kept, and filed, in one write.
async fn arrive(
    state: &Arc<State>,
    dsp: &str,
    phone: &str,
    id: &str,
    bytes: &'static [u8],
) -> Result<()> {
    let arrived = receive(state, dsp, upload(bytes).await).await?;
    let (at, phone, id) = (dsp.to_owned(), phone.to_owned(), id.to_owned());
    filing::changing(state, dsp, move |db| {
        let text = keep_reported(db, &at, &phone, &id, &arrived)?;
        Ok(((), vec![text]))
    })
    .await
}

#[tokio::test]
async fn bytes_sent_again_leave_those_before_them_kept_while_what_keeps_them_fails() {
    let (_root, state, dsp) = started();
    // The phone reports a text with a picture, filed as it comes, and sends its bytes.
    let at = dsp.clone();
    let (phone, file) = filing::changing(&state, &dsp, move |db| {
        let phone = phones(db, &at)?.remove(0).id;
        let text = reported("a", "+14255550199", now(), 1);
        let kept = db.phones_report(&at, &phone, vec![text])?.remove(0);
        Ok(((phone, kept.files[0].clone()), vec![kept.id]))
    })
    .await
    .unwrap();
    arrive(&state, &dsp, &phone, &file, b"door").await.unwrap();

    // It sends other bytes for it, and keeping whether its conversation waits then fails, so
    // nothing of it is kept: the picture's bytes are still those before, there to download.
    let schema = |sql: &'static str| {
        let at = dsp.clone();
        state.run(move |db| db.dsp(&at)?.exec(sql, []).map(drop))
    };
    schema(
        "CREATE TRIGGER settling_fails BEFORE UPDATE OF state ON communications_text_conversations \
         BEGIN SELECT RAISE(ABORT, 'settling fails'); END",
    )
    .await
    .unwrap();
    assert!(arrive(&state, &dsp, &phone, &file, b"dock").await.is_err());
    schema("DROP TRIGGER settling_fails").await.unwrap();
    let (at, id) = (dsp.clone(), file.clone());
    let bytes = state
        .read(move |db| {
            let sha256 = db.phones_file(&at, &id)?.unwrap().sha256.unwrap();
            Ok(std::fs::read(kept(db, &at).join(sha256)).ok())
        })
        .await
        .unwrap();
    assert_eq!(bytes.as_deref(), Some(&b"door"[..]));
    let shown = super::shown(&state, &dsp, &file, "usr_other").await;
    assert!(shown.unwrap().is_some(), "it downloads");
}

#[tokio::test]
async fn bytes_cut_short_as_they_arrive_leave_nothing_behind() {
    let (_root, db, dsp) = store();
    let folder = kept(&db, &dsp);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    // Two of four bytes come, then nothing more: told once the rest are waited on.
    let (told, waiting) = tokio::sync::oneshot::channel();
    let mut told = Some(told);
    let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"do"))]).chain(
        stream::poll_fn(move |_| {
            if let Some(told) = told.take() {
                let _ = told.send(());
            }
            Poll::<Option<std::io::Result<Bytes>>>::Pending
        }),
    );
    let upload = Upload::streamed(4, u64::MAX, chunks).await.unwrap();
    let mut receiving = Box::pin(receive(&state, &dsp, upload));
    tokio::select! {
        _ = &mut receiving => panic!("the rest never came"),
        _ = waiting => {}
    }
    // The upload ends there, as a request cancelled does.
    drop(receiving);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
}

#[test]
fn bytes_gone_once_upkeep_listed_them_went_already() {
    let (_root, db, dsp) = store();
    let folder = kept(&db, &dsp);
    // Upkeep lists an upload's bytes as they arrive; it's cancelled before upkeep looks at them.
    std::fs::write(folder.join(".arriving_cancelled"), b"do").unwrap();
    let listed = std::fs::read_dir(&folder).unwrap().next().unwrap().unwrap();
    std::fs::remove_file(listed.path()).unwrap();
    assert!(!unkept(&listed, &HashSet::new(), i64::MAX).unwrap());
}

/// The picture `bytes`, as `by` adds it for a text.
async fn add(state: &Arc<State>, dsp: &str, by: &str, bytes: &'static [u8]) -> String {
    let arrived = receive(state, dsp, upload(bytes).await).await.unwrap();
    let added = Added {
        by: by.into(),
        name: "door.png".into(),
        mime: "image/png".into(),
    };
    let at = dsp.to_owned();
    state
        .run_scoped(dsp, DOMAIN, move |db| keep_added(db, &at, added, &arrived))
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
    let outgoing = outgoing("(425) 555-0199", vec![sent.clone()], &owner);
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
            super::shown(&state, &dsp, &id, &viewer)
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
async fn a_picture_past_what_a_text_carries_is_refused_before_any_of_it_is_kept() {
    let (_root, db, dsp) = store();
    let folder = kept(&db, &dsp);
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
    let refused = receive(&state, &dsp, upload).await;
    assert_eq!(refused.err().unwrap().code.as_str(), "upload_too_large");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
}
