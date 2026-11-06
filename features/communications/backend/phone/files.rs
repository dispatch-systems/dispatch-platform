//! The bytes of the pictures and files texts carry, kept by their SHA-256 (`data/phones/files/`
//! until the object store has them), so two texts with one picture keep it once: arriving, taking
//! their place, kept in the object store and let go as `bytes` keeps them, with the files that
//! keep them. A video its phone keeps has only the picture of its first frame here until someone
//! plays it: asked for then (`want`), its phone sends its bytes, kept an hour. Upkeep lets go of
//! any bytes no file keeps, as bytes sent again in place of others, a video's an hour after they
//! came and a removal that failed leave, and of bytes left arriving, as a restart leaves.
use super::{
    LIMITS, live,
    model::{Added, FETCH_WAIT_MS, File},
    storage::{FileStore, ReportedFile, Wanting},
};
use crate::backend::{
    attachments,
    bytes::{Arrived, Folder},
};
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    server::http::{input::Reply, upload::Upload},
};
use std::{sync::Arc, time::Duration};

/// Where the DSP's pictures and files are kept, and which bytes its texts' files keep.
pub const BYTES: Folder = Folder {
    owner: "phones",
    keeps: <Store as FileStore>::phones_bytes_kept,
    kept: <Store as FileStore>::phones_kept_bytes,
};

/// The most bytes the picture of a video's first frame may be.
pub const PREVIEW_MOST: u64 = 256 * 1024;

/// Receives `bytes`, as they arrive, beside the DSP's files: refused before any arrive past what a
/// text carries (`LIMITS.largest`), however large an upload its route allows.
pub async fn receive(state: &Arc<State>, dsp: &str, bytes: Upload) -> Result<Arrived> {
    BYTES.receive(state, dsp, bytes, LIMITS.largest).await
}
/// Receives the picture of a video's first frame, as it arrives, beside the DSP's files: refused
/// before any arrive past `PREVIEW_MOST`.
pub async fn receive_preview(state: &Arc<State>, dsp: &str, bytes: Upload) -> Result<Arrived> {
    BYTES.receive(state, dsp, bytes, PREVIEW_MOST).await
}
/// Keeps `arrived` as the bytes of the file `id` of a text the phone `device` reported, in place of
/// any before them, its size what arrived: refused as `phones_file_not_found` for one of no text
/// of its own, its bytes let go. Inside the write that authorized the phone. The text it's of.
/// The bytes it replaces are upkeep's to let go of: the transaction keeping these may yet fail,
/// and keep the file's bytes before.
pub fn keep_reported(
    db: &Store,
    dsp: &str,
    device: &str,
    id: &str,
    arrived: &Arrived,
) -> Result<String> {
    BYTES.keep(db, dsp, arrived, || {
        db.phones_file_arrived(dsp, device, id, &arrived.sha256, arrived.size, db::now())
    })
}
/// Keeps `arrived` as the picture of the first frame of the video `id` the phone `device` reported
/// and keeps, in place of any before it, as `keep_reported` keeps a file's bytes. The text it's of.
pub fn keep_preview(
    db: &Store,
    dsp: &str,
    device: &str,
    id: &str,
    arrived: &Arrived,
) -> Result<String> {
    BYTES.keep(db, dsp, arrived, || {
        db.phones_preview_arrived(dsp, device, id, &arrived.sha256)
    })
}
/// Keeps `arrived` as the picture `added` for a text not sent yet: its bytes let go should it not
/// be kept. Inside a write.
pub fn keep_added(db: &Store, dsp: &str, added: Added, arrived: &Arrived) -> Result<File> {
    let file = ReportedFile {
        name: added.name,
        mime: added.mime,
        size: arrived.size,
        on_phone: false,
    };
    BYTES.keep(db, dsp, arrived, || {
        db.phones_add_file(dsp, &added.by, &file, &arrived.sha256, db::now())
    })
}
/// The DSP's picture or file `id` as kept, or the picture's `preview` when it has one, to answer
/// a page with, kept by the browser when the address asked for its `version`: none for one not
/// the DSP's, one whose bytes haven't come yet, and one added for a text not sent yet but by
/// `viewer`, whose alone it is until it's sent.
pub async fn shown(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    viewer: &str,
    version: Option<&str>,
    preview: bool,
) -> Result<Option<Reply>> {
    let (at, wanted) = (dsp.to_owned(), id.to_owned());
    let Some(stored) = state.read(move |db| db.phones_file(&at, &wanted)).await? else {
        return Ok(None);
    };
    // A preview is a picture's alone.
    if preview && !stored.file.mime.starts_with("image/") {
        return Ok(None);
    }
    let theirs = stored.text_id.is_some() || stored.added_by.as_deref() == Some(viewer);
    match &stored.file.sha256 {
        Some(sha256) if theirs => {
            let (name, mime) = (&stored.file.name, &stored.file.mime);
            let small = match preview {
                true => attachments::preview(state, BYTES, dsp, sha256, name, mime).await?,
                false => None,
            };
            let reply = match small {
                Some(small) => small,
                None => answer(state, dsp, sha256, name, mime).await?,
            };
            Ok(Some(attachments::as_asked(reply, version, sha256)))
        }
        _ => Ok(None),
    }
}
/// The picture of the first frame of the DSP's video `id`, of a text, to answer a page with: none
/// for one not the DSP's, or with none kept.
pub async fn preview(state: &Arc<State>, dsp: &str, id: &str) -> Result<Option<Reply>> {
    let (at, wanted) = (dsp.to_owned(), id.to_owned());
    let Some(stored) = state.read(move |db| db.phones_file(&at, &wanted)).await? else {
        return Ok(None);
    };
    match stored.preview_sha256 {
        Some(sha256) if stored.text_id.is_some() => Ok(Some(
            answer(
                state,
                dsp,
                &sha256,
                &format!("{}.jpg", stored.file.name),
                "image/jpeg",
            )
            .await?,
        )),
        _ => Ok(None),
    }
}
/// Asks for the bytes of the DSP's video `id`, of a text, that its phone keeps, as `authorize`
/// allows in the write that asks, waking its phone; then waits, up to `FETCH_WAIT_MS`, for them to
/// come, as the pages hear its text change, `authorize` asked again with each read after a wait:
/// whether they're kept. Refused
/// `communications_file_not_found` for no such video, `phones_offline` (503) while its phone is,
/// `phones_file_gone` (410) once it has it no more, and `platform_busy` (503) while the server
/// holds too many waits.
pub async fn fetch(
    state: &Arc<State>,
    dsp: &str,
    id: &str,
    authorize: impl Fn(&Store) -> Result<()> + Clone + Send + 'static,
) -> Result<bool> {
    let _slot = state
        .topics
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new("platform_busy", 503))?;
    // Subscribed before asking, so bytes that come between the ask and the wait still wake it.
    let mut listener = state.topics.subscribe(crate::backend::live::TOPIC, dsp);
    let (at, wanted, allowed) = (dsp.to_owned(), id.to_owned(), authorize.clone());
    // Bookkeeping: no cached read depends on when a video was asked for.
    let mut wanting = state
        .run_bookkeeping(move |db| {
            allowed(db)?;
            db.phones_want_file(&at, &wanted, db::now())
        })
        .await?
        .ok_or_else(|| Error::new("communications_file_not_found", 404))?;
    if wanting == Wanting::Asked {
        live::queued(state, dsp);
    }
    let until = tokio::time::Instant::now() + Duration::from_millis(FETCH_WAIT_MS as u64);
    while wanting == Wanting::Asked && tokio::time::Instant::now() < until {
        let _ = tokio::time::timeout_at(until, listener.changed()).await;
        let (at, wanted, allowed) = (dsp.to_owned(), id.to_owned(), authorize.clone());
        let stored = state
            .read(move |db| {
                allowed(db)?;
                db.phones_file(&at, &wanted)
            })
            .await?;
        wanting = match stored {
            Some(stored) if stored.file.kept => Wanting::Kept,
            Some(stored) if stored.file.gone => Wanting::Gone,
            Some(_) => Wanting::Asked,
            None => return Err(Error::new("communications_file_not_found", 404)),
        };
    }
    match wanting {
        Wanting::Kept => Ok(true),
        Wanting::Asked => Ok(false),
        Wanting::Offline => Err(Error::new("phones_offline", 503)),
        Wanting::Gone => Err(Error::new("phones_file_gone", 410)),
    }
}
/// The DSP's bytes kept by `sha256`, streamed, as a download of `name` and `mime`.
pub async fn answer(
    state: &Arc<State>,
    dsp: &str,
    sha256: &str,
    name: &str,
    mime: &str,
) -> Result<Reply> {
    let kept = BYTES
        .read(state, dsp, sha256)
        .await?
        .ok_or_else(|| Error::new("phones_file_not_found", 404))?;
    let body = axum::body::Body::from_stream(kept.chunks);
    Ok(Reply::download(mime, name, kept.length, body))
}

#[cfg(test)]
#[path = "../../tests/backend/phone/files.rs"]
mod tests;
