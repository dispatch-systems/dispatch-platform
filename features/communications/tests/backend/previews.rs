//! A picture's preview: no larger than a thread needs, its shape kept, upright as its camera
//! said to turn it, and none for a picture small enough already or bytes that aren't a picture;
//! those asking for one at once share bringing its picture down from the object store.
use super::*;
use crate::backend::phone::{files::BYTES, support::store};
use dispatch_core::testing::objects::Fake;
use image::{ImageEncoder, ImageFormat, RgbImage};
use reqwest::Method;

/// `bytes`, kept in a file of their own.
fn kept(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("bytes");
    std::fs::write(&path, bytes).unwrap();
    (folder, path)
}
/// A picture `width` by `height`, as `format`.
fn picture(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::new(width, height))
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}
/// The width and height of the JPEG `bytes`.
fn measured(bytes: &[u8]) -> (u32, u32) {
    let shown = image::load_from_memory_with_format(bytes, ImageFormat::Jpeg).unwrap();
    (shown.width(), shown.height())
}

#[test]
fn a_large_picture_previews_at_its_shape_no_larger_than_a_thread_needs() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg] {
        let (_folder, path) = kept(&picture(3_200, 1_600, format));
        assert_eq!(measured(&made(&path).unwrap()), (640, 320));
    }
    // A grey camera image stays grey through the scaled JPEG path.
    let mut grey = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(image::GrayImage::from_pixel(
        3_200,
        1_600,
        image::Luma([90]),
    ))
    .write_to(&mut grey, ImageFormat::Jpeg)
    .unwrap();
    let (_folder, path) = kept(&grey.into_inner());
    let preview = image::load_from_memory(&made(&path).unwrap())
        .unwrap()
        .to_rgb8();
    assert_eq!(preview.dimensions(), (640, 320));
    let [red, green, blue] = preview.get_pixel(0, 0).0;
    assert_eq!((red, green), (green, blue));
}

#[test]
fn a_photo_its_camera_turned_previews_upright() {
    // A phone held upright saves its photo sideways, with EXIF saying to turn it a quarter.
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .write_image(
            RgbImage::new(1_200, 800).as_raw(),
            1_200,
            800,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let turned: &[u8] = &[
        0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, // APP1, 34 bytes long
        b'M', b'M', 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08, // big-endian TIFF, its IFD at 8
        0x00, 0x01, // one entry
        0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00,
        0x00, // Orientation 6
        0x00, 0x00, 0x00, 0x00, // no IFD after
    ];
    let photo = [&jpeg[..2], turned, &jpeg[2..]].concat();
    let (_folder, path) = kept(&photo);
    assert_eq!(measured(&made(&path).unwrap()), (427, 640));
}

#[test]
fn a_picture_small_enough_or_bytes_that_arent_one_have_none() {
    for bytes in [
        picture(SIDE, 200, ImageFormat::Png),
        b"\x89PNG\r\n\x1a\n".to_vec(),
        b"not a picture".to_vec(),
    ] {
        let (_folder, path) = kept(&bytes);
        assert_eq!(made(&path), None);
    }
}

#[tokio::test]
async fn a_preview_asked_for_leaves_the_list_once_none_ask_cut_short_or_not() {
    let kept = std::path::PathBuf::from("/previews/asked-for-then-cut-short.jpg");
    let listed = || {
        UNDERWAY
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(&kept)
    };
    let making = Underway::join(&kept);
    let turn = making.turn().await;
    // Another asks meanwhile and waits its turn; the one making it finishes, then the other's
    // page goes before its turn comes.
    let waiting = Underway::join(&kept);
    let mut its_turn = Box::pin(waiting.turn());
    assert!(futures_util::FutureExt::now_or_never(&mut its_turn).is_none());
    drop((turn, making));
    assert!(listed(), "the other still asks");
    drop(its_turn);
    drop(waiting);
    assert!(!listed());
}

#[tokio::test]
async fn those_asking_for_a_preview_at_once_share_one_download_of_its_picture() {
    let fake = Fake::start().await;
    let (_root, db, dsp) = store();
    let mut config = db.config.clone();
    config.objects = Some(fake.store());
    drop(db);
    let state = State::new(config).unwrap();
    // Only the object store has the picture.
    let photo = picture(1_600, 800, ImageFormat::Png);
    let sha256 = crypto::sha(&photo);
    let key = format!("dsps/{dsp}/phones/{sha256}");
    fake.put(&key, &photo);
    let asking = (0..4).map(|_| preview(&state, BYTES, &dsp, &sha256));
    for shown in futures_util::future::join_all(asking).await {
        assert!(shown.unwrap().is_some());
    }
    assert_eq!(fake.asked(Method::GET, &key), 1);
}

#[tokio::test]
async fn cancelling_header_preparation_holds_its_turn_until_the_blocking_reader_finishes() {
    let slots = Arc::new(Semaphore::new(1));
    let turn = slots.clone().acquire_owned().await.unwrap();
    let (started, reading) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    let (finished, done) = tokio::sync::oneshot::channel();
    let reader = tokio::spawn(prepare(turn, move || {
        started.send(()).unwrap();
        released.recv().unwrap();
        let _ = finished.send(());
    }));
    reading.await.unwrap();
    reader.abort();
    assert!(reader.await.unwrap_err().is_cancelled());
    let next = slots.clone().try_acquire_owned();
    let still_held = next.is_err();
    drop(next);
    // Always finish the blocking worker before asserting, including on the original failure.
    release.send(()).unwrap();
    done.await.unwrap();
    let _finished = slots.acquire_owned().await.unwrap();
    assert!(
        still_held,
        "cancelled header worker released its admission turn before finishing"
    );
}
