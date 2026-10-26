//! The bytes of the pictures and files texts carry, kept in the DSP's own data
//! (`data/phones/files/`) by their SHA-256, so two texts with one picture keep it once. Bytes
//! arrive beside the folder's files, then take their place, and are let go, only under the
//! platform's write lock, with the files that keep them: so letting go never removes the bytes of
//! a file kept a moment before. Upkeep lets go of any bytes no file keeps, as a removal that
//! failed leaves, and of bytes left arriving, as a restart leaves.
use super::storage::{FileStore, ReportedFile};
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    foundation::crypto,
    server::http::{input::Reply, upload::Upload},
};
use dispatch_phones_contract::{Added, File};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long bytes may sit arriving, unchanged, before upkeep takes them for abandoned: far longer
/// than an upload may stall before it's given up, a minute.
const ABANDONED_MS: i64 = 60 * 60 * 1000;
/// What the name of bytes arriving begins with, beside those kept by their SHA-256.
const ARRIVING: &str = ".";

/// The folder the DSP's files are kept in.
fn folder(db: &Store, dsp: &str) -> Result<PathBuf> {
    let owner = db::private_dir(&db.area(dsp, "data")?.join("phones"))?;
    db::private_dir(&owner.join("files"))
}
/// Where bytes arrive, let go of should they never take their place among the DSP's files,
/// however receiving them ends.
struct Arriving(PathBuf);
impl Drop for Arriving {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
/// Bytes that arrived, beside the DSP's files until they take their place.
pub struct Arrived {
    pub sha256: String,
    pub size: i64,
    arriving: Arriving,
}
impl Arrived {
    /// Puts the bytes in their place among the DSP's files: inside the write that keeps the file
    /// they're for.
    fn place(&self, db: &Store, dsp: &str) -> Result<()> {
        std::fs::rename(&self.arriving.0, folder(db, dsp)?.join(&self.sha256))?;
        Ok(())
    }
}
/// Receives `bytes`, as they arrive, beside the DSP's files.
pub async fn receive(state: &Arc<State>, dsp: &str, bytes: Upload) -> Result<Arrived> {
    let at = dsp.to_owned();
    let folder = state.read(move |db| folder(db, &at)).await?;
    let arriving = Arriving(folder.join(format!("{ARRIVING}{}", crypto::id("arriving")?)));
    let mut written = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&arriving.0)
        .await?;
    let mut hash = Sha256::new();
    let mut size = 0i64;
    let mut bytes = Box::pin(bytes.stream());
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.map_err(|_| Error::new("upload_incomplete", 400))?;
        hash.update(&chunk);
        size += chunk.len() as i64;
        written.write_all(&chunk).await?;
    }
    written.sync_all().await?;
    Ok(Arrived {
        sha256: crypto::hex(&hash.finalize()),
        size,
        arriving,
    })
}
/// Keeps `arrived` as the bytes of the file `id` of a text the phone `device` reported, in place of
/// any before them, its size what arrived: refused as `phones_file_not_found` for one of no text
/// of its own, its bytes let go. Inside the write that authorized the phone.
pub fn keep_reported(
    db: &Store,
    dsp: &str,
    device: &str,
    id: &str,
    arrived: &Arrived,
) -> Result<()> {
    arrived.place(db, dsp)?;
    match db.phones_file_arrived(dsp, device, id, &arrived.sha256, arrived.size) {
        Ok(before) => before.map_or(Ok(()), |before| forget(db, dsp, &before)),
        Err(error) => {
            forget(db, dsp, &arrived.sha256)?;
            Err(error)
        }
    }
}
/// Keeps `arrived` as the picture `added` for a text not sent yet: its bytes let go should it not
/// be kept. Inside a write.
pub fn keep_added(db: &Store, dsp: &str, added: Added, arrived: &Arrived) -> Result<File> {
    arrived.place(db, dsp)?;
    let file = ReportedFile {
        name: added.name,
        mime: added.mime,
        size: arrived.size,
    };
    let kept = db.phones_add_file(dsp, &added.by, &file, &arrived.sha256, db::now());
    if kept.is_err() {
        forget(db, dsp, &arrived.sha256)?;
    }
    kept
}
/// Lets go of the DSP's bytes kept by `sha256`, unless a file is still kept by them: inside the
/// write that let go of the file.
pub fn forget(db: &Store, dsp: &str, sha256: &str) -> Result<()> {
    if db.phones_bytes_kept(dsp, sha256)? {
        return Ok(());
    }
    removed(&folder(db, dsp)?.join(sha256))
}
/// Lets go of the DSP's bytes no file keeps, and of bytes left arriving `ABANDONED_MS` unchanged,
/// each on its own: why any couldn't go, to be tried again the next time. Inside a write, so no
/// bytes take their place meanwhile.
pub fn reconcile(db: &Store, dsp: &str) -> Result<Vec<Error>> {
    let kept: HashSet<String> = db.phones_kept_bytes(dsp)?.into_iter().collect();
    let abandoned = db::now() - ABANDONED_MS;
    let mut failed = Vec::new();
    for entry in std::fs::read_dir(folder(db, dsp)?)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let unkept = if name.starts_with(ARRIVING) {
            changed_at(&entry)? < abandoned
        } else {
            !kept.contains(&name)
        };
        if unkept && let Err(error) = removed(&entry.path()) {
            failed.push(error);
        }
    }
    Ok(failed)
}
/// When the bytes at `entry` last changed, epoch milliseconds.
fn changed_at(entry: &std::fs::DirEntry) -> Result<i64> {
    let at = entry.metadata()?.modified()?.duration_since(UNIX_EPOCH);
    Ok(at.map_or(0, |at| at.as_millis() as i64))
}
/// Removes the bytes at `path`, once they're gone already too.
fn removed(path: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
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
    let at = dsp.to_owned();
    let path = state.read(move |db| folder(db, &at)).await?.join(sha256);
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| Error::new("phones_file_not_found", 404))?;
    let length = file.metadata().await.ok().map(|meta| meta.len());
    let chunks = futures_util::stream::unfold(file, |mut file| async move {
        let mut chunk = vec![0; 64 * 1024];
        match file.read(&mut chunk).await {
            Ok(0) => None,
            Ok(read) => {
                chunk.truncate(read);
                Some((Ok::<_, std::io::Error>(chunk), file))
            }
            Err(error) => Some((Err(error), file)),
        }
    });
    let body = axum::body::Body::from_stream(chunks);
    Ok(Reply::download(mime, name, length, body))
}

#[cfg(test)]
#[path = "../tests/backend/files.rs"]
mod tests;
