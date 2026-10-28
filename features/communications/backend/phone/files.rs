//! The bytes of the pictures and files texts carry, kept in the DSP's own data
//! (`data/phones/files/`) by their SHA-256, so two texts with one picture keep it once: arriving,
//! taking their place and let go as `bytes` keeps them, with the files that keep them. Upkeep lets
//! go of any bytes no file keeps, as bytes sent again in place of others and a removal that failed
//! leave, and of bytes left arriving, as a restart leaves.
use super::{
    LIMITS,
    model::{Added, File},
    storage::{FileStore, ReportedFile},
};
use crate::backend::bytes::{Arrived, Folder};
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    server::http::{input::Reply, upload::Upload},
};
use std::sync::Arc;

/// Where the DSP's pictures and files are kept, and which bytes its texts' files keep.
pub const BYTES: Folder = Folder {
    owner: "phones",
    keeps: <Store as FileStore>::phones_bytes_kept,
    kept: <Store as FileStore>::phones_kept_bytes,
};

/// Receives `bytes`, as they arrive, beside the DSP's files: refused before any arrive past what a
/// text carries (`LIMITS.largest`), however large an upload its route allows.
pub async fn receive(state: &Arc<State>, dsp: &str, bytes: Upload) -> Result<Arrived> {
    BYTES.receive(state, dsp, bytes, LIMITS.largest).await
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
        db.phones_file_arrived(dsp, device, id, &arrived.sha256, arrived.size)
    })
}
/// Keeps `arrived` as the picture `added` for a text not sent yet: its bytes let go should it not
/// be kept. Inside a write.
pub fn keep_added(db: &Store, dsp: &str, added: Added, arrived: &Arrived) -> Result<File> {
    let file = ReportedFile {
        name: added.name,
        mime: added.mime,
        size: arrived.size,
    };
    BYTES.keep(db, dsp, arrived, || {
        db.phones_add_file(dsp, &added.by, &file, &arrived.sha256, db::now())
    })
}
/// The DSP's picture or file `id` as kept, to answer a page with: none for one not the DSP's, one
/// whose bytes haven't come yet, and one added for a text not sent yet but by `viewer`, whose
/// alone it is until it's sent.
pub async fn shown(state: &Arc<State>, dsp: &str, id: &str, viewer: &str) -> Result<Option<Reply>> {
    let (at, wanted) = (dsp.to_owned(), id.to_owned());
    let Some(stored) = state.read(move |db| db.phones_file(&at, &wanted)).await? else {
        return Ok(None);
    };
    let theirs = stored.text_id.is_some() || stored.added_by.as_deref() == Some(viewer);
    match stored.sha256 {
        Some(sha256) if theirs => Ok(Some(
            answer(state, dsp, &sha256, &stored.file.name, &stored.file.mime).await?,
        )),
        _ => Ok(None),
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
