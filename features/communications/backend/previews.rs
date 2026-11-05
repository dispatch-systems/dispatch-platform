//! Small previews of the pictures messages carry, which a thread shows until one is opened: at
//! most `SIDE` pixels a side, turned upright, as a JPEG. Each is made from the picture's bytes the
//! first time it's asked for and kept in the DSP's cache, since the bytes make it again; those
//! asking for one at once share its making. A few are made at a time, so pictures arriving
//! together leave the server's processors and memory to everything else. A picture small enough
//! already, or one that can't be read as a picture, has none: it's shown as it is. A preview that
//! can't be kept is shown all the same.
use super::bytes::{Folder, Reading};
use dispatch_core::{
    Error, Result, State, foundation::crypto, foundation::observability, server::http::input::Reply,
};
use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits, Rgb, RgbImage, codecs::jpeg::JpegEncoder,
};
use serde_json::json;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, PoisonError},
};
use tokio::{
    io::AsyncWriteExt,
    sync::{OwnedMutexGuard, Semaphore},
};

/// What a preview is.
pub const MIME: &str = "image/jpeg";
/// The most pixels a preview has a side: twice what a thread shows, for sharp screens.
const SIDE: u32 = 640;
const QUALITY: u8 = 80;
/// A picture wider or taller has no preview, nor one whose reading would take more memory: far
/// past what a phone's camera makes.
const MOST_SIDE: u32 = 12_000;
const MOST_MEMORY: u64 = 512 * 1024 * 1024;
/// How many previews are made at once.
static MAKING: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(2)));
/// The previews asked for, by where each is kept.
static UNDERWAY: LazyLock<Mutex<HashMap<PathBuf, Asked>>> = LazyLock::new(Mutex::default);

/// A preview asked for: the turns at making it, and how many ask.
#[derive(Default)]
struct Asked {
    turns: Arc<tokio::sync::Mutex<()>>,
    asking: usize,
}

/// The preview of the DSP's picture kept by `sha256` in `folder`: none when it has none.
pub async fn preview(
    state: &Arc<State>,
    folder: Folder,
    dsp: &str,
    sha256: &str,
) -> Result<Option<Reading>> {
    let at = dsp.to_owned();
    let kept = state
        .read(move |db| folder.cache(db, &at, "previews"))
        .await
        .map(|previews| previews.join(format!("{sha256}.jpg")));
    if let Ok(kept) = &kept
        && let Some(reading) = opened(kept).await
    {
        return Ok(Some(reading));
    }
    let Some(bytes) = folder.located(state, dsp, sha256).await? else {
        return Ok(None);
    };
    let underway = kept.as_deref().ok().map(Underway::join);
    let _turn = match (&underway, &kept) {
        (Some(underway), Ok(kept)) => {
            let turn = underway.turn().await;
            // Made meanwhile, by another asking for it.
            if let Some(reading) = opened(kept).await {
                return Ok(Some(reading));
            }
            Some(turn)
        }
        _ => None,
    };
    let turn = MAKING
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Error::new("preview_unavailable", 503))?;
    // The turn goes with the making, which runs on once begun, even for a page gone meanwhile.
    let made = tokio::task::spawn_blocking(move || {
        let _turn = turn;
        made(&bytes)
    })
    .await
    .map_err(|_| Error::new("preview_unavailable", 503))?;
    let Some(jpeg) = made else {
        return Ok(None);
    };
    let not_kept = match &kept {
        Ok(kept) => stored(kept, &jpeg).await.err().map(|error| error.code),
        Err(error) => Some(error.code.clone()),
    };
    if let Some(code) = not_kept {
        observability::event(
            "error",
            "communications_preview_not_kept",
            json!({"dspId":dsp,"error":code}),
        );
    }
    Ok(Some(Reading::whole(jpeg)))
}

/// The preview of the DSP's picture kept by `sha256` in `folder`, as `mime` named `name`, for a
/// page to show: none for a file that isn't a picture, or a picture with none.
pub async fn answer(
    state: &Arc<State>,
    folder: Folder,
    dsp: &str,
    sha256: &str,
    name: &str,
    mime: &str,
) -> Result<Option<Reply>> {
    if !mime.starts_with("image/") {
        return Ok(None);
    }
    Ok(preview(state, folder, dsp, sha256).await?.map(|small| {
        Reply::download(
            MIME,
            name,
            small.length,
            axum::body::Body::from_stream(small.chunks),
        )
    }))
}

/// The preview kept at `path`, when there is one.
async fn opened(path: &Path) -> Option<Reading> {
    let file = tokio::fs::File::open(path).await.ok()?;
    Some(Reading::of(file).await)
}

/// Keeps `jpeg` as the preview at `kept`: written beside it, then put in its place, so it's never
/// read in part. Cut short, it leaves nothing behind.
async fn stored(kept: &Path, jpeg: &[u8]) -> Result<()> {
    let folder = kept
        .parent()
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    let arriving = Arriving(folder.join(format!(".{}", crypto::id("preview")?)));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&arriving.0)
        .await?;
    file.write_all(jpeg).await?;
    file.sync_all().await?;
    tokio::fs::rename(&arriving.0, kept).await?;
    Ok(())
}
/// A preview being written, let go of should it never take its place.
struct Arriving(PathBuf);
impl Drop for Arriving {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One asking for the preview at `kept`, among those asking for it at once, who take their turns
/// making it: the first makes it, those after find it made. Each counts while it asks, waiting
/// or cut short included, and the last one done takes the preview off the list.
struct Underway {
    kept: PathBuf,
    turns: Arc<tokio::sync::Mutex<()>>,
}
impl Underway {
    /// Asks for the preview at `kept`, among any others asking for it.
    fn join(kept: &Path) -> Self {
        let mut underway = UNDERWAY.lock().unwrap_or_else(PoisonError::into_inner);
        let asked = underway.entry(kept.to_owned()).or_default();
        asked.asking += 1;
        Self {
            kept: kept.to_owned(),
            turns: asked.turns.clone(),
        }
    }
    /// Waits for those before it to be done.
    async fn turn(&self) -> OwnedMutexGuard<()> {
        self.turns.clone().lock_owned().await
    }
}
impl Drop for Underway {
    fn drop(&mut self) {
        let mut underway = UNDERWAY.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(asked) = underway.get_mut(&self.kept) {
            asked.asking -= 1;
            if asked.asking == 0 {
                underway.remove(&self.kept);
            }
        }
    }
}

/// The preview of the picture `bytes`, as a JPEG: none for one that fits already, or that can't
/// be read as a picture within the limits.
fn made(bytes: &Path) -> Option<Vec<u8>> {
    let mut reader = ImageReader::open(bytes).ok()?.with_guessed_format().ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MOST_SIDE);
    limits.max_image_height = Some(MOST_SIDE);
    limits.max_alloc = Some(MOST_MEMORY);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width <= SIDE && height <= SIDE {
        return None;
    }
    if decoder.total_bytes() > MOST_MEMORY {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut picture = DynamicImage::from_decoder(decoder).ok()?;
    picture.apply_orientation(orientation);
    let small = flattened(&picture.thumbnail(SIDE, SIDE));
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
        .encode_image(&small)
        .ok()?;
    Some(jpeg)
}

/// `picture` on white, as a JPEG has no see-through parts.
fn flattened(picture: &DynamicImage) -> RgbImage {
    if !picture.color().has_alpha() {
        return picture.to_rgb8();
    }
    let rgba = picture.to_rgba8();
    RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let over = |c: u8| ((u16::from(c) * u16::from(a) + 255 * (255 - u16::from(a))) / 255) as u8;
        Rgb([over(r), over(g), over(b)])
    })
}

#[cfg(test)]
#[path = "../tests/backend/previews.rs"]
mod tests;
