//! Copying to the object store never holds up the cache's limit: it's kept between batches of
//! copies, and on its own while an upload stalls.
use super::*;
use crate::backend::phone::support::{kept, store};
use dispatch_core::{
    foundation::{config::ObjectStore, crypto},
    testing::objects::Fake,
};
use std::{path::Path, time::UNIX_EPOCH};

/// How many bytes the files in `folder` hold.
fn held(folder: &Path) -> u64 {
    std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum()
}

#[tokio::test]
async fn the_cache_keeps_within_its_limit_while_an_upload_stalls() {
    let fake = Fake::start().await;
    let (_root, db, dsp) = store();
    // A batch of pictures and one more, four bytes each, waiting in the DSP's data.
    let data = kept(&db, &dsp);
    let mut waiting: Vec<String> = (0..=BATCH)
        .map(|n| {
            let bytes = format!("{n:04}");
            let sha256 = crypto::sha(&bytes);
            std::fs::write(data.join(&sha256), bytes).unwrap();
            sha256
        })
        .collect();
    waiting.sort();
    let last = format!("dsps/{dsp}/phones/{}", waiting[BATCH]);
    let stalled = fake.hold(&last);
    let mut config = db.config.clone();
    config.objects = Some(ObjectStore {
        only: true,
        ..fake.store()
    });
    drop(db);
    let state = State::new(config).unwrap();
    let cache = state
        .config
        .root
        .join("dsps")
        .join(&dsp)
        .join("cache/phones");
    let copying = tokio::spawn({
        let (state, dsp) = (state.clone(), dsp.clone());
        async move { copied(&state, &dsp, 8).await }
    });
    stalled.arrived().await;
    // The first batch became cached copies, kept within the limit before the next was copied.
    assert!(held(&cache.join("files")) <= 8);
    // A preview read long ago, past the limit, goes on the hour while the upload stalls.
    let preview = cache.join("previews/read-long-ago.jpg");
    std::fs::create_dir_all(preview.parent().unwrap()).unwrap();
    std::fs::write(&preview, [0; 100]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&preview)
        .unwrap()
        .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_000))
        .unwrap();
    bounded(&state, 8).await;
    assert!(!preview.exists());
    assert!(!copying.is_finished());
    stalled.go();
    copying.await.unwrap();
    assert!(fake.object(&last).is_some());
}
