//! The bytes texts carry, as they arrive: a reported file's are kept with their own size, and
//! those cut short leave nothing behind.
use super::*;
use crate::backend::{
    storage::{DOMAIN, TextStore},
    support::{kept, paired, reported, store},
};
use axum::body::Bytes;
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
    state
        .run_scoped(dsp.clone(), DOMAIN, move |db| {
            keep_reported(db, &at, &phone.id, &file, &arrived)
        })
        .await
        .unwrap();
    let at = dsp.clone();
    let changes = state
        .read(move |db| db.phones_changes(&at, 0))
        .await
        .unwrap();
    let file = &changes.texts[0].files[0];
    assert_eq!((file.size, file.kept), (4, true));
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
