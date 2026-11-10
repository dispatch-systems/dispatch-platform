//! Small previews of the pictures messages carry, which a thread shows until one is opened: at
//! most `SIDE` pixels a side, turned upright, as a JPEG. Each is made from the picture's bytes the
//! first time it's asked for and kept in the DSP's cache, since the bytes make it again, for as
//! long as a file keeps the bytes (`Folder::reconcile`) and the server's limit of cached copies
//! leaves it (`bytes::bound`); those asking for one at once share its making, and bringing its
//! picture down from the object store when the server has no copy of it. A few are made at a
//! time, so pictures arriving together leave the server's processors and memory to everything
//! else. A picture small enough already, one that can't be read as a picture, or one only the
//! object store has while the DSP's cache can't be used to bring it down, has none: it's shown as
//! it is. A preview that can't be kept is shown all the same.
use super::bytes::{self, Arriving, Folder, Reading};
use dispatch_core::{Error, Result, State, foundation::crypto, foundation::observability};
use image::{
    DynamicImage, GrayImage, ImageDecoder, ImageFormat, ImageReader, Limits, Rgb, RgbImage,
    codecs::jpeg::JpegEncoder,
};
use serde_json::json;
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, PoisonError},
};
use tokio::{
    io::AsyncWriteExt,
    sync::{OwnedMutexGuard, OwnedSemaphorePermit, Semaphore},
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
/// Aggregate decoded pixel storage admitted at once, in MiB. Decoder scratch and the upright
/// copy also take memory; this bounds their inputs rather than treating every photo alike.
static PIXELS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(512)));
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
    // In its turn, so those asking at once share bringing the picture down.
    let Some(bytes) = folder.located(state, dsp, sha256).await? else {
        return Ok(None);
    };
    let turn = MAKING
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| Error::new("preview_unavailable", 503))?;
    // Header reads do no pixel decoding. Keep the making slot while waiting for this picture's
    // share of the aggregate pixel budget, so waiting requests retain no decoded image.
    let ((bytes, memory), turn) = prepare(turn, move || {
        let memory = pixel_memory(&bytes);
        (bytes, memory)
    })
    .await?;
    let Some(memory) = memory else {
        return Ok(None);
    };
    let pixels = PIXELS
        .clone()
        .acquire_many_owned(memory)
        .await
        .map_err(|_| Error::new("preview_unavailable", 503))?;
    // Both permits go with the making, which runs on once begun, even for a page gone.
    let made = tokio::task::spawn_blocking(move || {
        let (_turn, _pixels) = (turn, pixels);
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

/// The preview kept at `path`, when there is one, marked read now, as the cache keeps what was
/// read most recently.
async fn opened(path: &Path) -> Option<Reading> {
    let file = bytes::read_now(path).await?;
    Some(Reading::of(file).await)
}

/// Keeps `jpeg` as the preview at `kept`: written beside it, then put in its place, so it's never
/// read in part. Cut short, it leaves nothing behind.
async fn stored(kept: &Path, jpeg: &[u8]) -> Result<()> {
    let folder = kept
        .parent()
        .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
    let (arriving, mut file) =
        Arriving::create(folder.join(format!(".{}", crypto::id("preview")?))).await?;
    file.write_all(jpeg).await?;
    file.sync_all().await?;
    tokio::fs::rename(arriving.path(), kept).await?;
    Ok(())
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

/// Runs header preparation under its making turn, returning it for the later decode.
async fn prepare<T: Send + 'static>(
    turn: OwnedSemaphorePermit,
    read: impl FnOnce() -> T + Send + 'static,
) -> Result<(T, OwnedSemaphorePermit)> {
    // The worker owns the turn from admission until its work finishes, even if the
    // requesting future is cancelled while waiting for it.
    tokio::task::spawn_blocking(move || (read(), turn))
        .await
        .map_err(|_| Error::new("preview_unavailable", 503))
}

/// The decoded pixel storage a picture needs, rounded up to MiB; none for a picture that
/// needs no preview or exceeds its limits. Only headers are read.
fn pixel_memory(bytes: &Path) -> Option<u32> {
    let decoder = reader(bytes)?.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width <= SIDE && height <= SIDE || decoder.total_bytes() > MOST_MEMORY {
        return None;
    }
    Some(decoder.total_bytes().div_ceil(1024 * 1024).max(1) as u32)
}

/// A picture's reader, with the same limits for its header and its later decoding.
fn reader(bytes: &Path) -> Option<ImageReader<BufReader<File>>> {
    let mut reader = ImageReader::open(bytes).ok()?.with_guessed_format().ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MOST_SIDE);
    limits.max_image_height = Some(MOST_SIDE);
    limits.max_alloc = Some(MOST_MEMORY);
    reader.limits(limits);
    Some(reader)
}

/// The preview of the picture `bytes`, as a JPEG: none for one that fits already, or that can't
/// be read as a picture within the limits.
fn made(bytes: &Path) -> Option<Vec<u8>> {
    let reader = reader(bytes)?;
    let jpeg = reader.format() == Some(ImageFormat::Jpeg);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width <= SIDE && height <= SIDE {
        return None;
    }
    if decoder.total_bytes() > MOST_MEMORY {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    // JPEG can decode directly at a half, quarter or eighth of its size. Other formats and
    // JPEG variants the scaled decoder does not support keep the existing decoder path.
    let mut picture = if jpeg {
        scaled_jpeg(bytes).or_else(|| DynamicImage::from_decoder(decoder).ok())?
    } else {
        DynamicImage::from_decoder(decoder).ok()?
    };
    picture.apply_orientation(orientation);
    let small = flattened(&picture.thumbnail(SIDE, SIDE));
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, QUALITY)
        .encode_image(&small)
        .ok()?;
    Some(jpeg)
}

/// A sequential JPEG's pixels at the smallest supported size still covering the preview.
/// Progressive and lossless pictures use the existing bounded decoder: their full coefficient
/// storage does not benefit from the row decoder's reduced output size.
fn scaled_jpeg(bytes: &Path) -> Option<DynamicImage> {
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(File::open(bytes).ok()?));
    decoder.set_max_decoding_buffer_size(MOST_MEMORY as usize);
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    if info.coding_process != jpeg_decoder::CodingProcess::DctSequential {
        return None;
    }
    let (width, height) = decoder.scale(SIDE as u16, SIDE as u16).ok()?;
    let pixels = decoder.decode().ok()?;
    let (width, height) = (u32::from(width), u32::from(height));
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => {
            GrayImage::from_raw(width, height, pixels).map(DynamicImage::ImageLuma8)
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8)
        }
        // Keep the existing decoder's colour conversion and precision for other variants.
        _ => None,
    }
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
