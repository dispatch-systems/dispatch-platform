//! The bytes texts carry, as they arrive: a reported file's are kept with their own size.
use super::*;
use crate::backend::{
    storage::{DOMAIN, TextStore},
    support::{paired, reported, store},
};
use axum::body::Bytes;
use futures_util::stream;

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
