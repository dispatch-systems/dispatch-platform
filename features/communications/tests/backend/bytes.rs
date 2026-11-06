//! Bytes kept by their SHA-256: those gone once upkeep listed them went already. With an object
//! store, bytes wait in the DSP's data until it has them; they stay there too, for a release that
//! reads only the disk, until they may leave it, then become the cached copy, and come back from
//! it once the cache let them go, checked and cached as a download ends; it being down loses
//! nothing; what no file keeps goes from it too, and only the DSP's own; and the cache keeps
//! within its limit, letting go of what was read longest ago first. Bytes leave the disk only once
//! the bucket the server now has has them. A video's first frame, and its bytes once played, are
//! kept as a picture's are, the bytes going from it once let go of. A cache that can't be used
//! never keeps bytes from being read, nor those no file keeps from going; nor does a DSP's data
//! that can't be used keep the object store's bytes from being read. Without one, bytes stay in
//! the DSP's data.
use super::*;
use crate::backend::phone::{
    files::{BYTES, answer, preview, shown},
    storage::{FileStore, ReportedFile, TextStore},
    support::{kept, paired, reported, store},
};
use axum::response::IntoResponse;
use dispatch_core::{
    foundation::config::ObjectStore,
    testing::{self, objects::Fake},
};
use reqwest::Method;
use std::time::SystemTime;

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

/// A platform with one DSP and its server's state, `objects` its object store when there's one,
/// with a picture of `bytes` added for a text not sent yet at 1,000, its bytes in the DSP's data:
/// and their SHA-256, and the picture.
fn added(
    objects: Option<ObjectStore>,
    bytes: &[u8],
) -> (impl Drop, Arc<State>, String, String, String) {
    let (root, db, dsp) = store();
    let sha256 = crypto::sha(bytes);
    std::fs::write(kept(&db, &dsp).join(&sha256), bytes).unwrap();
    let picture = ReportedFile {
        name: "door.jpg".into(),
        mime: "image/jpeg".into(),
        size: bytes.len() as i64,
        on_phone: false,
    };
    let owner = testing::a_user(&db);
    let picture = db
        .phones_add_file(&dsp, &owner, &picture, &sha256, 1_000)
        .unwrap();
    let mut config = db.config.clone();
    config.objects = objects;
    drop(db);
    (root, State::new(config).unwrap(), dsp, sha256, picture.id)
}
/// `fake`, as a server's object store once bytes may leave its disk.
fn only(fake: &Fake) -> ObjectStore {
    ObjectStore {
        only: true,
        ..fake.store()
    }
}
/// Copies what waits in the DSP's data, as upkeep does: why any couldn't be.
async fn copied(state: &Arc<State>, dsp: &str) -> Vec<Error> {
    let waiting = BYTES.waiting(state, dsp).await.unwrap();
    BYTES.copy(state, dsp, &waiting).await
}
/// Where the DSP's pictures are kept in its data, and their cached copies in its cache.
fn folders(state: &State, dsp: &str) -> (PathBuf, PathBuf) {
    let at = state.config.root.join("dsps").join(dsp);
    (at.join("data/phones/files"), at.join("cache/phones/files"))
}
/// The DSP's picture `sha256`, as it's read.
async fn read(state: &Arc<State>, dsp: &str, sha256: &str) -> Option<Vec<u8>> {
    let reading = BYTES.read(state, dsp, sha256).await.unwrap()?;
    let chunks: Vec<_> = reading.chunks.collect().await;
    Some(
        chunks
            .into_iter()
            .flat_map(|chunk| chunk.unwrap())
            .collect(),
    )
}
/// The DSP's picture `sha256` as a phone downloads it, over HTTP/1 from a server of its own: its
/// bytes, or why they didn't all come.
async fn downloaded(state: &Arc<State>, dsp: &str, sha256: &str) -> reqwest::Result<Bytes> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (state, dsp, sha256) = (state.clone(), dsp.to_owned(), sha256.to_owned());
    let route = axum::routing::get(move || async move {
        answer(&state, &dsp, &sha256, "door.jpg", "image/jpeg")
            .await
            .unwrap()
    });
    let serving = tokio::spawn(async move {
        axum::serve(listener, axum::Router::new().route("/", route))
            .await
            .unwrap();
    });
    let download = async {
        reqwest::get(format!("http://{address}/"))
            .await?
            .bytes()
            .await
    };
    let bytes = download.await;
    serving.abort();
    bytes
}
/// The store's folder in the DSP's cache, taken by a file, as a disk gone wrong leaves.
fn unusable(state: &State, dsp: &str) {
    let cache = state.config.root.join("dsps").join(dsp).join("cache");
    std::fs::write(db::private_dir(&cache).unwrap().join("phones"), b"").unwrap();
}
/// The server's cache, every copy let go of.
fn emptied(state: &State) {
    assert!(bound(&state.config.root, &["phones"], 0).is_empty());
}

#[tokio::test]
async fn bytes_copied_become_the_cached_copy_and_come_back_once_the_cache_let_them_go() {
    let fake = Fake::start().await;
    let (_root, state, dsp, sha256, _) = added(Some(only(&fake)), b"door");
    let (data, cache) = folders(&state, &dsp);
    assert!(copied(&state, &dsp).await.is_empty());
    let key = format!("dsps/{dsp}/phones/{sha256}");
    assert_eq!(fake.object(&key).unwrap(), b"door");
    assert!(
        !data.join(&sha256).exists(),
        "the object store keeps them now"
    );
    assert!(cache.join(&sha256).exists(), "as the cached copy");
    // Read, the cached copy is marked read now, for the cache to keep it over those read before.
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let marked = || {
        std::fs::metadata(cache.join(&sha256))
            .unwrap()
            .modified()
            .unwrap()
    };
    std::fs::File::options()
        .write(true)
        .open(cache.join(&sha256))
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
    assert!(marked() > long_ago);
    // Let go of by the cache, they come back from the object store, cached again as they come.
    emptied(&state);
    assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
    assert!(cache.join(&sha256).exists());
    // A preview's bytes come back to the server the same way.
    emptied(&state);
    let located = BYTES.located(&state, &dsp, &sha256).await.unwrap();
    assert_eq!(located, Some(cache.join(&sha256)));
    assert_eq!(std::fs::read(cache.join(&sha256)).unwrap(), b"door");
}

#[tokio::test]
async fn a_download_from_the_object_store_is_checked_and_cached_as_it_ends() {
    let fake = Fake::start().await;
    let (_root, state, dsp, sha256, _) = added(Some(only(&fake)), b"door");
    let (_, cache) = folders(&state, &dsp);
    assert!(copied(&state, &dsp).await.is_empty());
    emptied(&state);
    // The download ends as its last byte goes, its length told: cached all the same.
    assert_eq!(downloaded(&state, &dsp, &sha256).await.unwrap(), "door");
    assert_eq!(std::fs::read(cache.join(&sha256)).unwrap(), b"door");
    // Other bytes in their place, as many, never come whole, nor are cached.
    emptied(&state);
    fake.put(&format!("dsps/{dsp}/phones/{sha256}"), b"dock");
    assert!(downloaded(&state, &dsp, &sha256).await.is_err());
    assert!(!cache.join(&sha256).exists());
}

#[tokio::test]
async fn the_object_store_down_loses_nothing_and_its_copy_waits_for_the_next_time() {
    let fake = Fake::start().await;
    let (_root, state, dsp, sha256, _) = added(Some(only(&fake)), b"door");
    let (data, _) = folders(&state, &dsp);
    fake.down(true);
    assert_eq!(copied(&state, &dsp).await.len(), 1);
    assert!(data.join(&sha256).exists(), "still in the DSP's data");
    assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
    fake.down(false);
    assert!(copied(&state, &dsp).await.is_empty());
    assert!(
        fake.object(&format!("dsps/{dsp}/phones/{sha256}"))
            .is_some()
    );
}

#[tokio::test]
async fn bytes_copied_stay_in_the_dsps_data_until_they_may_leave_the_disk() {
    let fake = Fake::start().await;
    let (_root, state, dsp, sha256, _) = added(Some(fake.store()), b"door");
    let (data, cache) = folders(&state, &dsp);
    let key = format!("dsps/{dsp}/phones/{sha256}");
    assert!(copied(&state, &dsp).await.is_empty());
    assert_eq!(fake.object(&key).unwrap(), b"door");
    // Kept on disk, past the cache's limit, where a release that reads only the disk looks.
    emptied(&state);
    assert_eq!(std::fs::read(data.join(&sha256)).unwrap(), b"door");
    let mut config = state.config.clone();
    config.objects = None;
    drop(state);
    let disk_only = State::new(config.clone()).unwrap();
    assert_eq!(read(&disk_only, &dsp, &sha256).await.unwrap(), b"door");
    drop(disk_only);
    // Copied once, not again each hour.
    config.objects = Some(fake.store());
    let state = State::new(config.clone()).unwrap();
    assert!(copied(&state, &dsp).await.is_empty());
    assert_eq!(fake.asked(Method::PUT, &key), 1);
    drop(state);
    // Once they may leave it, they become the cached copy, without being copied again.
    config.objects = Some(only(&fake));
    let state = State::new(config).unwrap();
    assert!(copied(&state, &dsp).await.is_empty());
    assert!(!data.join(&sha256).exists());
    assert_eq!(std::fs::read(cache.join(&sha256)).unwrap(), b"door");
    assert_eq!(fake.asked(Method::PUT, &key), 1);
}

#[tokio::test]
async fn bytes_leave_the_disk_only_once_the_bucket_now_in_place_has_them() {
    let (first, second) = (Fake::start().await, Fake::start().await);
    let (_root, state, dsp, sha256, _) = added(Some(first.store()), b"door");
    let (data, _) = folders(&state, &dsp);
    let key = format!("dsps/{dsp}/phones/{sha256}");
    assert!(copied(&state, &dsp).await.is_empty());
    assert_eq!(first.object(&key).unwrap(), b"door");
    // The server starts with another bucket in its place, and bytes may leave its disk: the first
    // bucket having them is no reason to.
    let mut config = state.config.clone();
    config.objects = Some(only(&second));
    drop(state);
    let state = State::new(config).unwrap();
    assert!(copied(&state, &dsp).await.is_empty());
    assert_eq!(second.object(&key).unwrap(), b"door");
    assert!(!data.join(&sha256).exists());
    emptied(&state);
    assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
}

#[tokio::test]
async fn a_videos_first_frame_and_bytes_played_come_back_from_the_object_store_until_let_go() {
    let fake = Fake::start().await;
    let (_root, db, dsp) = store();
    let phone = paired(&db, &dsp, "key", "2065550100");
    let mut text = reported("mms-1", "4255550199", 3_000, 1);
    text.files[0] = ReportedFile {
        name: "defect.mp4".into(),
        mime: "video/mp4".into(),
        size: 5,
        on_phone: true,
    };
    let video = db.phones_report(&dsp, &phone.id, vec![text]).unwrap()[0].files[0].clone();
    // Its first frame's picture, then, once someone played it, its bytes.
    let (frame, bytes) = (crypto::sha(b"frame"), crypto::sha(b"video"));
    std::fs::write(kept(&db, &dsp).join(&frame), b"frame").unwrap();
    std::fs::write(kept(&db, &dsp).join(&bytes), b"video").unwrap();
    db.phones_preview_arrived(&dsp, &phone.id, &video, &frame)
        .unwrap();
    db.phones_file_arrived(&dsp, &phone.id, &video, &bytes, 5, 11_000)
        .unwrap();
    let mut config = db.config.clone();
    config.objects = Some(only(&fake));
    drop(db);
    let state = State::new(config).unwrap();
    assert!(copied(&state, &dsp).await.is_empty());
    // Let go of by the cache, both come back from the object store: the frame where a thread
    // shows it, the bytes where the video plays.
    emptied(&state);
    let body = |reply: Reply| axum::body::to_bytes(reply.into_response().into_body(), usize::MAX);
    let shown_frame = preview(&state, &dsp, &video).await.unwrap().unwrap();
    assert_eq!(body(shown_frame).await.unwrap(), "frame");
    let played = shown(&state, &dsp, &video, "anyone", None, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body(played).await.unwrap(), "video");
    // An hour after they came its bytes are let go of, from the object store too; its frame stays.
    let at = dsp.clone();
    state
        .run(move |db| {
            db.phones_let_go_fetched(&at, 11_001)?;
            BYTES.reconcile(db, &at)
        })
        .await
        .unwrap();
    assert!(BYTES.release(&state, &dsp).await.is_empty());
    assert_eq!(fake.keys(), [format!("dsps/{dsp}/phones/{frame}")]);
    assert_eq!(read(&state, &dsp, &bytes).await, None);
    assert_eq!(read(&state, &dsp, &frame).await.unwrap(), b"frame");
}

#[tokio::test]
async fn a_cache_that_cant_be_used_never_keeps_bytes_from_being_read() {
    let fake = Fake::start().await;
    for objects in [None, Some(only(&fake))] {
        let (_root, state, dsp, sha256, picture) = added(objects, b"door");
        let (data, _) = folders(&state, &dsp);
        unusable(&state, &dsp);
        assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
        let located = BYTES.located(&state, &dsp, &sha256).await.unwrap();
        assert_eq!(located, Some(data.join(&sha256)));
        // Only the object store has them: they come down uncached, as the picture itself where a
        // thread asks for its preview.
        if state.objects.is_some() {
            fake.put(&format!("dsps/{dsp}/phones/{sha256}"), b"door");
            std::fs::remove_file(data.join(&sha256)).unwrap();
            assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
            let owner = state.read(|db| Ok(testing::a_user(db))).await.unwrap();
            let shown = shown(&state, &dsp, &picture, &owner, None, true)
                .await
                .unwrap()
                .unwrap();
            let body = axum::body::to_bytes(shown.into_response().into_body(), usize::MAX);
            assert_eq!(body.await.unwrap(), "door");
        }
    }
}

#[tokio::test]
async fn a_dsps_data_that_cant_be_used_never_keeps_the_object_stores_bytes_from_being_read() {
    let fake = Fake::start().await;
    let mut photo = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::new(1_600, 800))
        .write_to(&mut photo, image::ImageFormat::Png)
        .unwrap();
    let photo = photo.into_inner();
    let (_root, state, dsp, _, picture) = added(Some(only(&fake)), &photo);
    let (data, _) = folders(&state, &dsp);
    assert!(copied(&state, &dsp).await.is_empty());
    emptied(&state);
    // Only the object store has the picture, and a file is where the DSP's folder of them was.
    std::fs::remove_dir(&data).unwrap();
    std::fs::write(&data, b"").unwrap();
    let owner = state.read(|db| Ok(testing::a_user(db))).await.unwrap();
    let body = |reply: Reply| axum::body::to_bytes(reply.into_response().into_body(), usize::MAX);
    let whole = shown(&state, &dsp, &picture, &owner, None, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body(whole).await.unwrap(), photo);
    // Its preview is made of it brought down again.
    emptied(&state);
    let small = shown(&state, &dsp, &picture, &owner, None, true)
        .await
        .unwrap()
        .unwrap();
    let small = body(small).await.unwrap();
    assert_eq!(
        image::guess_format(&small).unwrap(),
        image::ImageFormat::Jpeg
    );
    // Bytes none other has: why the DSP's data can't be used.
    let unread = BYTES.read(&state, &dsp, &crypto::sha(b"dock")).await;
    assert_eq!(unread.err().unwrap().code, "unsafe_storage_path");
}

#[tokio::test]
async fn a_cache_that_cant_be_used_never_keeps_unkept_bytes_from_going() {
    let fake = Fake::start().await;
    for objects in [None, Some(only(&fake))] {
        let (_root, state, dsp, sha256, _) = added(objects, b"door");
        let (data, _) = folders(&state, &dsp);
        unusable(&state, &dsp);
        // Beside the picture never sent, an upload abandoned long ago.
        let abandoned = data.join(".arriving_abandoned");
        std::fs::write(&abandoned, b"do").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&abandoned)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
            .unwrap();
        let at = dsp.clone();
        let failed = state
            .run(move |db| {
                db.phones_tidy_files(&at, 2_000)?;
                BYTES.reconcile(db, &at)
            })
            .await
            .unwrap();
        assert_eq!(failed.len(), 1, "the cache, tried again next time");
        assert!(!data.join(&sha256).exists() && !abandoned.exists());
    }
}

#[tokio::test]
async fn without_an_object_store_bytes_stay_in_the_dsps_data() {
    let (_root, state, dsp, sha256, _) = added(None, b"door");
    let (data, cache) = folders(&state, &dsp);
    assert!(copied(&state, &dsp).await.is_empty());
    assert!(BYTES.release(&state, &dsp).await.is_empty());
    emptied(&state);
    assert!(data.join(&sha256).exists());
    assert!(!cache.join(&sha256).exists());
    assert_eq!(read(&state, &dsp, &sha256).await.unwrap(), b"door");
}

#[tokio::test]
async fn what_no_file_keeps_goes_from_the_object_store_once_a_day_and_only_the_dsps_own() {
    let fake = Fake::start().await;
    let (_root, state, dsp, sha256, _) = added(Some(only(&fake)), b"door");
    let (_, cache) = folders(&state, &dsp);
    assert!(copied(&state, &dsp).await.is_empty());
    // Another DSP has the same bytes, and the DSP the bytes of a file let go of before.
    let other = format!("dsp_{}", "b".repeat(32));
    let theirs = format!("dsps/{other}/phones/{sha256}");
    fake.put(&theirs, b"door");
    let dock = crypto::sha(b"dock");
    fake.put(&format!("dsps/{dsp}/phones/{dock}"), b"dock");
    assert_eq!(read(&state, &other, &dock).await, None, "not the other's");
    // The picture never sent is let go of: its cached copy with upkeep, then its object.
    let at = dsp.clone();
    state
        .run(move |db| {
            db.phones_tidy_files(&at, 2_000)?;
            BYTES.reconcile(db, &at)
        })
        .await
        .unwrap();
    assert!(!cache.join(&sha256).exists());
    assert!(BYTES.release(&state, &dsp).await.is_empty());
    assert_eq!(fake.keys(), [theirs]);
    // Listed once today, they aren't listed again.
    fake.put(&format!("dsps/{dsp}/phones/{dock}"), b"dock");
    assert!(BYTES.release(&state, &dsp).await.is_empty());
    assert_eq!(fake.keys().len(), 2);
}

#[test]
fn the_cache_lets_go_of_what_was_read_longest_ago_and_never_of_a_dsps_data() {
    let root = tempfile::tempdir().unwrap();
    let dsp = root
        .path()
        .join("dsps")
        .join(format!("dsp_{}", "a".repeat(32)));
    let copy = |folder: &str, name: &str, read_at: u64| {
        let path = dsp.join(folder).join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0; 10]).unwrap();
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(read_at);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at)
            .unwrap();
        path
    };
    // A store's copies and previews, and another store's, read at different times; and bytes in
    // the DSP's data, older than any, larger than the limit.
    let oldest = copy("cache/phones/files", "a", 1_000);
    let older = copy("cache/communications/previews", "b.jpg", 2_000);
    let newer = copy("cache/phones/previews", "c.jpg", 3_000);
    let newest = copy("cache/communications/files", "d", 4_000);
    let data = copy("data/phones/files", "e", 0);
    std::fs::write(&data, [0; 100]).unwrap();
    assert!(bound(root.path(), &["phones", "communications"], 25).is_empty());
    assert!(!oldest.exists() && !older.exists());
    assert!(newer.exists() && newest.exists() && data.exists());
}
