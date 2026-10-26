//! The bytes of the pictures and files texts carry, kept in the DSP's own data
//! (`data/phones/files/`) by their SHA-256, so two texts with one picture keep it once. Bytes
//! arrive beside the folder's files, then take their place, and are let go, only under the
//! platform's write lock, with the files that keep them: so letting go never removes the bytes of
//! a file kept a moment before.
use super::storage::FileStore;
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    foundation::crypto,
    server::http::{input::Reply, upload::Upload},
};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The most one picture or file may be: as much as a phone sends in one text and more.
pub const LARGEST: u64 = 20 * 1024 * 1024;

/// The folder the DSP's files are kept in.
fn folder(db: &Store, dsp: &str) -> Result<PathBuf> {
    let owner = db::private_dir(&db.area(dsp, "data")?.join("phones"))?;
    db::private_dir(&owner.join("files"))
}
/// Bytes that arrived, beside the DSP's files until they take their place.
pub struct Arrived {
    pub sha256: String,
    pub size: i64,
    arriving: PathBuf,
}
impl Arrived {
    /// Puts the bytes in their place among the DSP's files: inside the write that keeps the file
    /// they're for.
    pub fn place(&self, db: &Store, dsp: &str) -> Result<()> {
        std::fs::rename(&self.arriving, folder(db, dsp)?.join(&self.sha256))?;
        Ok(())
    }
}
impl Drop for Arrived {
    /// Bytes that never took their place are let go.
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.arriving);
    }
}
/// Receives `bytes`, as they arrive, beside the DSP's files.
pub async fn receive(state: &Arc<State>, dsp: &str, bytes: Upload) -> Result<Arrived> {
    let at = dsp.to_owned();
    let folder = state.read(move |db| folder(db, &at)).await?;
    let arriving = folder.join(format!(".{}", crypto::id("arriving")?));
    let mut written = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&arriving)
        .await?;
    let mut hash = Sha256::new();
    let mut size = 0i64;
    let mut bytes = Box::pin(bytes.stream());
    let copied: Result<()> = async {
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.map_err(|_| Error::new("upload_incomplete", 400))?;
            hash.update(&chunk);
            size += chunk.len() as i64;
            written.write_all(&chunk).await?;
        }
        written.sync_all().await?;
        Ok(())
    }
    .await;
    if let Err(error) = copied {
        let _ = tokio::fs::remove_file(&arriving).await;
        return Err(error);
    }
    Ok(Arrived {
        sha256: crypto::hex(&hash.finalize()),
        size,
        arriving,
    })
}
/// Lets go of the DSP's bytes kept by `sha256`, unless a file is still kept by them: inside the
/// write that let go of the file.
pub fn forget(db: &Store, dsp: &str, sha256: &str) -> Result<()> {
    if db.phones_bytes_kept(dsp, sha256)? {
        return Ok(());
    }
    match std::fs::remove_file(folder(db, dsp)?.join(sha256)) {
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
