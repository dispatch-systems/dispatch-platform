//! Bytes kept by their SHA-256 in a folder of the DSP's own data, so bytes many files carry are
//! kept once: each store of files has its folder (`Folder`), and says which bytes its files keep.
//! Bytes arrive beside the folder's, then take their place, and are let go, only under the
//! platform's write lock, with the files that keep them: so letting go never removes the bytes of
//! a file kept a moment before. Upkeep lets go of any bytes no file keeps, and of bytes left
//! arriving, as a restart leaves.
use axum::body::Bytes;
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    ensure,
    foundation::crypto,
    server::http::upload::Upload,
};
use futures_util::{Stream, StreamExt, stream};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::UNIX_EPOCH,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long bytes may sit arriving, unchanged, before upkeep takes them for abandoned: far longer
/// than an upload may stall before it's given up, a minute.
const ABANDONED_MS: i64 = 60 * 60 * 1000;
/// What the name of bytes arriving begins with, beside those kept by their SHA-256.
const ARRIVING: &str = ".";
/// How many bytes are read at a time as they're streamed.
const CHUNK: usize = 64 * 1024;

/// A store's folder of bytes in each DSP's data, `data/<owner>/files/`, and which bytes its files
/// keep.
#[derive(Clone, Copy)]
pub struct Folder {
    /// The folder of the DSP's data its `files/` is in.
    pub owner: &'static str,
    /// Whether any of its files is kept by a SHA-256.
    pub keeps: fn(&Store, &str, &str) -> Result<bool>,
    /// The SHA-256 of every bytes its files keep.
    pub kept: fn(&Store, &str) -> Result<Vec<String>>,
}

/// Where bytes arrive, let go of should they never take their place among the DSP's, however
/// receiving them ends.
struct Arriving(PathBuf);
impl Drop for Arriving {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
/// Bytes that arrived, beside the DSP's until they take their place.
pub struct Arrived {
    pub sha256: String,
    pub size: i64,
    arriving: Arriving,
}
impl Arrived {
    /// The first of the bytes, up to `most`.
    pub async fn head(&self, most: usize) -> Result<Vec<u8>> {
        let mut file = tokio::fs::File::open(&self.arriving.0).await?;
        let mut head = vec![0; most];
        let read = file.read(&mut head).await?;
        head.truncate(read);
        Ok(head)
    }
}

/// Bytes kept, as they're read.
pub struct Reading {
    /// How many there are, when that can be told.
    pub length: Option<u64>,
    pub chunks: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
}

impl Folder {
    /// The DSP's folder, made private.
    fn path(&self, db: &Store, dsp: &str) -> Result<PathBuf> {
        let owner = db::private_dir(&db.area(dsp, "data")?.join(self.owner))?;
        db::private_dir(&owner.join("files"))
    }
    /// Receives `bytes`, as they arrive, beside the DSP's: refused before any arrive past `most`,
    /// however large an upload its route allows.
    pub async fn receive(
        &self,
        state: &Arc<State>,
        dsp: &str,
        bytes: Upload,
        most: u64,
    ) -> Result<Arrived> {
        ensure(bytes.length <= most, "upload_too_large", 413)?;
        let (folder, at) = (*self, dsp.to_owned());
        let folder = state.read(move |db| folder.path(db, &at)).await?;
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
    /// Puts `arrived` in its place among the DSP's bytes, and keeps the file they're for with
    /// `keep`: inside the write that keeps it. Should `keep` fail, the bytes are let go unless a
    /// file keeps them still, as one kept before: what keeps them is read in that write, so a
    /// transaction `keep` is part of mustn't have kept a file by them already.
    pub fn keep<T>(
        &self,
        db: &Store,
        dsp: &str,
        arrived: &Arrived,
        keep: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        std::fs::rename(
            &arrived.arriving.0,
            self.path(db, dsp)?.join(&arrived.sha256),
        )?;
        keep().or_else(|error| {
            self.let_go(db, dsp, &arrived.sha256)?;
            Err(error)
        })
    }
    /// Lets go of the DSP's bytes kept by `sha256`, unless a file keeps them still: inside a write.
    fn let_go(&self, db: &Store, dsp: &str, sha256: &str) -> Result<()> {
        if (self.keeps)(db, dsp, sha256)? {
            return Ok(());
        }
        removed(&self.path(db, dsp)?.join(sha256))
    }
    /// Lets go of the DSP's bytes no file keeps, and of bytes left arriving `ABANDONED_MS`
    /// unchanged, each on its own: why any couldn't be looked at or go, to be tried again the next
    /// time. Inside a write, so no bytes take their place meanwhile.
    pub fn reconcile(&self, db: &Store, dsp: &str) -> Result<Vec<Error>> {
        let kept: HashSet<String> = (self.kept)(db, dsp)?.into_iter().collect();
        let abandoned = db::now() - ABANDONED_MS;
        let mut failed = Vec::new();
        for entry in std::fs::read_dir(self.path(db, dsp)?)? {
            let tidied = entry.map_err(Error::from).and_then(|entry| {
                if unkept(&entry, &kept, abandoned)? {
                    removed(&entry.path())
                } else {
                    Ok(())
                }
            });
            if let Err(error) = tidied {
                failed.push(error);
            }
        }
        Ok(failed)
    }
    /// The DSP's bytes kept by `sha256`, as they're read: none when they can't be opened, as bytes
    /// not kept.
    pub async fn read(
        &self,
        state: &Arc<State>,
        dsp: &str,
        sha256: &str,
    ) -> Result<Option<Reading>> {
        let (folder, at) = (*self, dsp.to_owned());
        let path = state
            .read(move |db| folder.path(db, &at))
            .await?
            .join(sha256);
        let Ok(file) = tokio::fs::File::open(path).await else {
            return Ok(None);
        };
        let length = file.metadata().await.ok().map(|meta| meta.len());
        let chunks = stream::unfold(file, |mut file| async move {
            let mut chunk = vec![0; CHUNK];
            match file.read(&mut chunk).await {
                Ok(0) => None,
                Ok(read) => {
                    chunk.truncate(read);
                    Some((Ok(Bytes::from(chunk)), file))
                }
                Err(error) => Some((Err(error), file)),
            }
        });
        Ok(Some(Reading {
            length,
            chunks: Box::pin(chunks),
        }))
    }
}

/// Whether the bytes at `entry` are to go: no file in `kept` keeps them, or, still arriving, they
/// last changed before `abandoned`. Bytes gone once listed, as an upload cancelled meanwhile
/// leaves, went already.
fn unkept(entry: &std::fs::DirEntry, kept: &HashSet<String>, abandoned: i64) -> Result<bool> {
    let name = entry.file_name().to_string_lossy().into_owned();
    if !name.starts_with(ARRIVING) {
        return Ok(!kept.contains(&name));
    }
    match changed_at(entry) {
        Ok(at) => Ok(at < abandoned),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
/// When the bytes at `entry` last changed, epoch milliseconds.
fn changed_at(entry: &std::fs::DirEntry) -> std::io::Result<i64> {
    let at = entry.metadata()?.modified()?.duration_since(UNIX_EPOCH);
    Ok(at.map_or(0, |at| at.as_millis() as i64))
}
/// Removes the bytes at `path`, once they're gone already too.
fn removed(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "../tests/backend/bytes.rs"]
mod tests;
