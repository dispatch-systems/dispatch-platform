//! Bytes kept by their SHA-256 in a folder of the DSP's own data, so bytes many files carry are
//! kept once: each store of files has its folder (`Folder`), and says which bytes its files keep.
//! Bytes arrive beside the folder's, then take their place, and are let go, only under the
//! platform's write lock, with the files that keep them: so letting go never removes the bytes of
//! a file kept a moment before. Upkeep lets go of any bytes no file keeps, with what they make
//! again in the DSP's cache, as their previews, and of bytes or what they make left arriving, as
//! a restart leaves.
//!
//! With an object store (`State::objects`), it is where bytes are kept for good: new bytes wait in
//! the DSP's data, and so in its backups, until hourly upkeep copies them there, each to its key
//! `dsps/<dsp>/<owner>/<sha256>`. Until bytes may leave the server's disk (`Objects::only`, set
//! once the release a rollback would return to reads the object store), they stay in the DSP's
//! data too, marked copied in its cache by the bucket that has them
//! (`cache/<owner>/copied-<bucket>/`, `Objects::identity`), so a release that reads only the disk
//! finds them all, and another bucket in its place has them copied to it again. Once they may,
//! those the bucket has become the server's cached copy, in the DSP's cache
//! (`cache/<owner>/files/`), which backups leave out. Bytes are read from the DSP's data, else its
//! cache, else the object store, coming down into the cache as they're read: a folder that can't
//! be used is passed over, and they come uncached when it's the cache. Reading a cached copy marks
//! it read, and upkeep keeps every DSP's cached copies, with what they make again, within the
//! server's limit (`bound`), letting go of those read longest ago first; it never lets go of bytes
//! in a DSP's data. Upkeep lets go of the bytes no file keeps in the object store too, once a day,
//! as a listing is paid for. Without one, bytes stay in the DSP's data as they always did. Reading
//! bytes the server has never waits on the object store, and it being down or slow loses nothing:
//! what upkeep couldn't copy waits in the DSP's data for the next hour.
use axum::body::Bytes;
use dispatch_core::{
    Error, Result, State,
    db::{self, Store, objects::Objects},
    ensure,
    foundation::crypto,
    server::http::{input::Reply, upload::Upload},
};
use futures_util::{Stream, StreamExt, stream};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long bytes may sit arriving, unchanged, before upkeep takes them for abandoned: far longer
/// than an upload may stall before it's given up, a minute.
const ABANDONED_MS: i64 = 60 * 60 * 1000;
/// What the name of bytes arriving begins with, beside those kept by their SHA-256.
const ARRIVING: &str = ".";
/// How many bytes are read at a time as they're streamed.
const CHUNK: usize = 64 * 1024;
/// How long upkeep waits between listings of what a DSP keeps in the object store, which are paid
/// for by the page.
const LISTED_MS: i64 = 24 * 60 * 60 * 1000;
/// The folder of a store's cached copies, in its folder of the DSP's cache.
const COPIES: &str = "files";
/// What begins the name of each folder marking the bytes in the DSP's data an object store has,
/// each by an empty file named by their SHA-256, while they stay on the server's disk too: one
/// for each bucket, named for it (`marks`), so bytes leave the disk only once the bucket the
/// server now has has them; in the store's folder of the DSP's cache, so bytes restored from a
/// backup are copied again.
const COPIED: &str = "copied-";
/// What marks, by when it last changed, when upkeep last listed what a store of the DSP keeps in
/// the object store, in its folder of the DSP's cache.
const LISTED: &str = "listed";

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
impl Reading {
    /// `bytes`, already read.
    pub fn whole(bytes: Vec<u8>) -> Self {
        Self {
            length: Some(bytes.len() as u64),
            chunks: Box::pin(stream::once(async move { Ok(Bytes::from(bytes)) })),
        }
    }
    /// The bytes of `file`, open, as they're read.
    pub async fn of(file: tokio::fs::File) -> Self {
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
        Self {
            length,
            chunks: Box::pin(chunks),
        }
    }
}

impl Folder {
    /// The DSP's folder, made private.
    fn path(&self, db: &Store, dsp: &str) -> Result<PathBuf> {
        let owner = db::private_dir(&db.area(dsp, "data")?.join(self.owner))?;
        db::private_dir(&owner.join("files"))
    }
    /// The store's folder in the DSP's cache, made private.
    fn in_cache(&self, db: &Store, dsp: &str) -> Result<PathBuf> {
        db::private_dir(&db.area(dsp, db::CACHE)?.join(self.owner))
    }
    /// The DSP's folder `name` of what its bytes make again, in its cache, made private.
    pub fn cache(&self, db: &Store, dsp: &str, name: &str) -> Result<PathBuf> {
        db::private_dir(&self.in_cache(db, dsp)?.join(name))
    }
    /// The DSP's folder of bytes in its data.
    async fn data(&self, state: &Arc<State>, dsp: &str) -> Result<PathBuf> {
        let (folder, at) = (*self, dsp.to_owned());
        state.read(move |db| folder.path(db, &at)).await
    }
    /// The DSP's folder `name` in its cache, made private.
    async fn cached(&self, state: &Arc<State>, dsp: &str, name: &str) -> Result<PathBuf> {
        let (folder, at, name) = (*self, dsp.to_owned(), name.to_owned());
        state.read(move |db| folder.cache(db, &at, &name)).await
    }
    /// Where the DSP's bytes kept by `sha256` are, on this server: in its data, looked for before
    /// its cache, else brought down from the object store into its cache when only it has them.
    /// None when they aren't kept, or only the object store has them and the cache can't be used;
    /// why its data can't be used when neither its cache nor the object store has them.
    pub async fn located(
        &self,
        state: &Arc<State>,
        dsp: &str,
        sha256: &str,
    ) -> Result<Option<PathBuf>> {
        let kept = self.data(state, dsp).await.map(|kept| kept.join(sha256));
        if let Ok(kept) = &kept
            && tokio::fs::try_exists(kept).await.unwrap_or(false)
        {
            return Ok(Some(kept.clone()));
        }
        let Ok(cached) = self.cached(state, dsp, COPIES).await else {
            return Ok(None);
        };
        let path = cached.join(sha256);
        if read_now(&path).await.is_some() {
            return Ok(Some(path));
        }
        let Some(mut fetched) = self.fetched(state, dsp, Some(&cached), sha256).await? else {
            return kept.map(|_| None);
        };
        while let Some(chunk) = fetched.chunks.next().await {
            chunk?;
        }
        Ok(tokio::fs::try_exists(&path)
            .await
            .unwrap_or(false)
            .then_some(path))
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
        let folder = self.data(state, dsp).await?;
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
    /// Lets go of the DSP's bytes no file keeps, with their cached copies, what else they make
    /// again and what marks them copied in its cache, and of any of these left arriving
    /// `ABANDONED_MS` unchanged, each on its own, those in its data even when its cache can't be
    /// used, and those in its cache even when its data can't: why any couldn't be looked at or
    /// go, to be tried again the next time. Inside a write, so no bytes take their place
    /// meanwhile. Their copies in the object store go once a day (`release`).
    pub fn reconcile(&self, db: &Store, dsp: &str) -> Result<Vec<Error>> {
        let kept: HashSet<String> = (self.kept)(db, dsp)?.into_iter().collect();
        let abandoned = db::now() - ABANDONED_MS;
        let mut failed = Vec::new();
        let mut folders = Vec::new();
        match self.path(db, dsp) {
            Ok(data) => folders.push(data),
            Err(error) => failed.push(error),
        }
        match self.in_cache(db, dsp) {
            Ok(cache) => {
                for made in listed(&cache, &mut failed) {
                    if made.file_type().is_ok_and(|kind| kind.is_dir()) {
                        folders.push(made.path());
                    }
                }
            }
            Err(error) => failed.push(error),
        }
        for folder in folders {
            for entry in listed(&folder, &mut failed) {
                let tidied = unkept(&entry, &kept, abandoned).and_then(|unkept| {
                    if unkept {
                        removed(&entry.path())
                    } else {
                        Ok(())
                    }
                });
                if let Err(error) = tidied {
                    failed.push(error);
                }
            }
        }
        Ok(failed)
    }
    /// The DSP's bytes kept by `sha256`, as they're read: from its data, opened before its cache
    /// is looked at, else its cache, else the object store, uncached when the cache can't be used.
    /// None when none has them, as bytes not kept; why its data can't be used when it's the only
    /// one that might.
    pub async fn read(
        &self,
        state: &Arc<State>,
        dsp: &str,
        sha256: &str,
    ) -> Result<Option<Reading>> {
        let kept = self.data(state, dsp).await;
        if let Ok(kept) = &kept
            && let Ok(file) = tokio::fs::File::open(kept.join(sha256)).await
        {
            return Ok(Some(Reading::of(file).await));
        }
        let cached = self.cached(state, dsp, COPIES).await.ok();
        if let Some(cached) = &cached
            && let Some(file) = read_now(&cached.join(sha256)).await
        {
            return Ok(Some(Reading::of(file).await));
        }
        match self.fetched(state, dsp, cached.as_deref(), sha256).await? {
            Some(fetched) => Ok(Some(fetched)),
            None => kept.map(|_| None),
        }
    }
    /// The DSP's bytes kept by `sha256` in the object store, as they come down, copied into
    /// `cached`, when there's a cache to copy them to, as they do: the copy takes its place there
    /// once they've all come, and only as the bytes `sha256` names, else they end in an error.
    /// Each chunk is given once the next has come, and the last once the copy took its place, or
    /// as that error: a reader that stops once it has as many bytes as it was told, as a download
    /// does, has them checked and cached all the same. None without an object store, or when it
    /// has none by `sha256`.
    async fn fetched(
        &self,
        state: &Arc<State>,
        dsp: &str,
        cached: Option<&Path>,
        sha256: &str,
    ) -> Result<Option<Reading>> {
        let Some(objects) = &state.objects else {
            return Ok(None);
        };
        let Some(fetched) = objects.get(dsp, self.owner, sha256).await? else {
            return Ok(None);
        };
        let copy = match cached {
            Some(cached) => {
                let arriving =
                    Arriving(cached.join(format!("{ARRIVING}{}", crypto::id("arriving")?)));
                tokio::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&arriving.0)
                    .await
                    .ok()
                    .map(|file| Caching {
                        file,
                        arriving,
                        place: cached.join(sha256),
                    })
            }
            None => None,
        };
        let coming = Coming {
            chunks: fetched.chunks,
            hash: Sha256::new(),
            copy,
            sha256: sha256.to_owned(),
        };
        let chunks = stream::unfold(Some((coming, None)), |coming| async move {
            let (mut coming, mut held) = coming?;
            loop {
                match coming.chunks.next().await {
                    Some(Ok(chunk)) => {
                        coming.copied(&chunk).await;
                        if let Some(before) = held.replace(chunk) {
                            return Some((Ok(before), Some((coming, held))));
                        }
                    }
                    Some(Err(error)) => return Some((Err(error), None)),
                    None => {
                        return match coming.finished().await {
                            Ok(()) => held.map(|last| (Ok(last), None)),
                            Err(error) => Some((Err(error), None)),
                        };
                    }
                }
            }
        });
        Ok(Some(Reading {
            length: fetched.length,
            chunks: Box::pin(chunks),
        }))
    }
    /// The SHA-256 of each of the DSP's bytes in its data `copy` has to see to, in order: while
    /// bytes may not leave the server's disk, only those not marked copied by its bucket. None
    /// without an object store.
    pub async fn waiting(&self, state: &Arc<State>, dsp: &str) -> Result<Vec<String>> {
        let Some(objects) = &state.objects else {
            return Ok(Vec::new());
        };
        let mut waiting = named(&self.data(state, dsp).await?).await?;
        if !objects.only() {
            let copied: HashSet<String> = named(&self.cached(state, dsp, &marks(objects)).await?)
                .await?
                .into_iter()
                .collect();
            waiting.retain(|sha256| !copied.contains(sha256));
        }
        waiting.sort();
        Ok(waiting)
    }
    /// Copies the DSP's bytes `waiting` in its data to the object store, each on its own, unless
    /// they're marked copied by its bucket already: why any couldn't be, to be tried again the
    /// next time. Until bytes may leave the server's disk (`Objects::only`), they stay in the
    /// DSP's data, marked copied; once they may, they become the cached copy, for the cache to let
    /// go of in its turn. Nothing without an object store.
    pub async fn copy(&self, state: &Arc<State>, dsp: &str, waiting: &[String]) -> Vec<Error> {
        let Some(objects) = &state.objects else {
            return Vec::new();
        };
        let folders = async {
            Ok::<_, Error>((
                self.data(state, dsp).await?,
                self.cached(state, dsp, COPIES).await?,
                self.cached(state, dsp, &marks(objects)).await?,
            ))
        };
        let (kept, cached, copied) = match folders.await {
            Ok(folders) => folders,
            Err(error) => return vec![error],
        };
        let mut failed = Vec::new();
        for sha256 in waiting {
            let (path, marked) = (kept.join(sha256), copied.join(sha256));
            let done = async {
                if !tokio::fs::try_exists(&marked).await? {
                    objects.put(dsp, self.owner, sha256, &path).await?;
                    tokio::fs::write(&marked, b"").await?;
                }
                if objects.only() {
                    // Let go of meanwhile, they go from the object store with the next listing.
                    match tokio::fs::rename(&path, cached.join(sha256)).await {
                        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                            return Err(error.into());
                        }
                        _ => removed(&marked)?,
                    }
                }
                Ok(())
            };
            if let Err(error) = done.await {
                failed.push(error);
            }
        }
        failed
    }
    /// Lets go of the DSP's bytes in the object store no file keeps, each on its own, at most once
    /// a day (`LISTED_MS`), as listing them is paid for by the page: why any couldn't be listed or
    /// go, to be tried again the next hour. Nothing without an object store. Bytes a file keeps
    /// once they're listed come back to the DSP's data as they arrive, and each is marked copied
    /// no more before it goes, for `copy` to copy them again: so `copy` follows it.
    pub async fn release(&self, state: &Arc<State>, dsp: &str) -> Vec<Error> {
        let Some(objects) = &state.objects else {
            return Vec::new();
        };
        let (folder, at, marks) = (*self, dsp.to_owned(), marks(objects));
        let folders = state
            .read(move |db| Ok((folder.in_cache(db, &at)?, folder.cache(db, &at, &marks)?)))
            .await;
        let (listed, copied) = match folders {
            Ok((cached, copied)) => (cached.join(LISTED), copied),
            Err(error) => return vec![error],
        };
        let since = tokio::fs::metadata(&listed)
            .await
            .and_then(|meta| meta.modified())
            .map_or(0, millis);
        if db::now() - since < LISTED_MS {
            return Vec::new();
        }
        let unkept = async {
            let stored = objects.listed(dsp, self.owner).await?;
            let (folder, at) = (*self, dsp.to_owned());
            state
                .read(move |db| {
                    let kept: HashSet<String> = (folder.kept)(db, &at)?.into_iter().collect();
                    Ok(stored
                        .into_iter()
                        .filter(|sha256| !kept.contains(sha256))
                        .collect::<Vec<_>>())
                })
                .await
        };
        let mut failed = Vec::new();
        match unkept.await {
            Ok(unkept) => {
                for sha256 in unkept {
                    let gone = async {
                        removed(&copied.join(&sha256))?;
                        objects.delete(dsp, self.owner, &sha256).await
                    };
                    if let Err(error) = gone.await {
                        failed.push(error);
                    }
                }
            }
            Err(error) => failed.push(error),
        }
        if failed.is_empty()
            && let Err(error) = tokio::fs::write(&listed, b"").await
        {
            failed.push(error.into());
        }
        failed
    }
}

/// Keeps the server's cached copies of every DSP's bytes of the stores `owners`, and what they
/// make again, within `most` bytes all together, under `root`: letting go of those read longest
/// ago first, each on its own, and never of bytes in a DSP's data, nor of what marks them copied.
/// Why any couldn't be looked at or go, to be tried again the next time.
pub fn bound(root: &Path, owners: &[&str], most: u64) -> Vec<Error> {
    let mut failed = Vec::new();
    let mut cached = Vec::new();
    for dsp in listed(&root.join("dsps"), &mut failed) {
        if !db::identifier(&dsp.file_name().to_string_lossy(), "dsp_") {
            continue;
        }
        for owner in owners {
            let folder = dsp.path().join(db::CACHE).join(owner);
            for kind in listed(&folder, &mut failed) {
                if !kind.file_type().is_ok_and(|kind| kind.is_dir())
                    || kind.file_name().to_string_lossy().starts_with(COPIED)
                {
                    continue;
                }
                for entry in listed(&kind.path(), &mut failed) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let copy = entry.metadata().map_err(Error::from).and_then(|meta| {
                        Ok((meta.is_file(), millis(meta.modified()?), meta.len()))
                    });
                    match copy {
                        Ok((true, read, size)) if !name.starts_with(ARRIVING) => {
                            cached.push((read, size, entry.path()));
                        }
                        Ok(_) => {}
                        Err(error) => failed.push(error),
                    }
                }
            }
        }
    }
    cached.sort();
    let mut total: u64 = cached.iter().map(|(_, size, _)| size).sum();
    for (_, size, path) in cached {
        if total <= most {
            break;
        }
        match removed(&path) {
            Ok(()) => total -= size,
            Err(error) => failed.push(error),
        }
    }
    failed
}

/// The version of the bytes `sha256` names, as an address carries it so the browser keeps them:
/// enough of it that no two of a DSP's bytes share one.
pub fn version(sha256: &str) -> String {
    sha256.chars().take(16).collect()
}
/// `reply`, the bytes `sha256`, kept by the browser when the address asked for their version,
/// `asked`: another version comes at another address.
pub fn as_asked(reply: Reply, asked: Option<&str>, sha256: &str) -> Reply {
    if asked.is_some_and(|asked| asked == version(sha256)) {
        reply.kept()
    } else {
        reply
    }
}

/// Bytes coming down from the object store, and their copy arriving in the cache, when there's
/// one, until a write to it fails.
struct Coming {
    chunks: Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>,
    hash: Sha256,
    copy: Option<Caching>,
    sha256: String,
}
/// A copy arriving in the cache, and where it takes its place.
struct Caching {
    file: tokio::fs::File,
    arriving: Arriving,
    place: PathBuf,
}
impl Coming {
    /// Copies `chunk`, as it came: should the copy fail, the bytes still come, uncopied.
    async fn copied(&mut self, chunk: &Bytes) {
        self.hash.update(chunk);
        if let Some(copy) = &mut self.copy
            && copy.file.write_all(chunk).await.is_err()
        {
            self.copy = None;
        }
    }
    /// Puts the copy in its place, once all came and they're the bytes `sha256` names: an error
    /// otherwise, which the bytes come to in its place.
    async fn finished(self) -> std::io::Result<()> {
        if crypto::hex(&self.hash.finalize()) != self.sha256 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "object_store_bytes_changed",
            ));
        }
        if let Some(copy) = self.copy
            && copy.file.sync_all().await.is_ok()
        {
            let _ = tokio::fs::rename(&copy.arriving.0, &copy.place).await;
        }
        Ok(())
    }
}

/// The name of a store's folder in the DSP's cache marking the bytes the bucket of `objects` has.
fn marks(objects: &Objects) -> String {
    format!("{COPIED}{}", objects.identity())
}
/// The cached copy at `path`, open, marked read now, so the cache keeps it over those read longer
/// ago: none when there's none.
pub async fn read_now(path: &Path) -> Option<tokio::fs::File> {
    let path = path.to_owned();
    let now = UNIX_EPOCH + Duration::from_millis(db::now().max(0) as u64);
    let file = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(path)?;
        // Read all the same, should it not be marked.
        let _ = file.set_modified(now);
        Ok::<_, std::io::Error>(file)
    })
    .await
    .ok()?
    .ok()?;
    Some(tokio::fs::File::from_std(file))
}
/// What's in `folder`, none when there's no folder: why any of it couldn't be listed in `failed`.
fn listed(folder: &Path, failed: &mut Vec<Error>) -> Vec<std::fs::DirEntry> {
    match std::fs::read_dir(folder) {
        Ok(entries) => entries
            .filter_map(|entry| entry.map_err(|error| failed.push(error.into())).ok())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            failed.push(error.into());
            Vec::new()
        }
    }
}
/// The SHA-256 of each of the bytes in `folder`: what's arriving, or named otherwise, left out.
async fn named(folder: &Path) -> Result<Vec<String>> {
    let mut named = Vec::new();
    let mut entries = tokio::fs::read_dir(folder).await?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 64
            && name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            named.push(name);
        }
    }
    Ok(named)
}
/// `at`, epoch milliseconds.
fn millis(at: std::time::SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .map_or(0, |at| at.as_millis() as i64)
}

/// Whether the bytes at `entry`, or what they make, are to go: no file in `kept` keeps the bytes
/// they're named by, before any `.` as in a preview's `<sha256>.jpg`; or, still arriving, they
/// last changed before `abandoned`. Bytes gone once listed, as an upload cancelled meanwhile
/// leaves, went already.
fn unkept(entry: &std::fs::DirEntry, kept: &HashSet<String>, abandoned: i64) -> Result<bool> {
    let name = entry.file_name().to_string_lossy().into_owned();
    if !name.starts_with(ARRIVING) {
        let bytes = name
            .split_once('.')
            .map_or(name.as_str(), |(bytes, _)| bytes);
        return Ok(!kept.contains(bytes));
    }
    match changed_at(entry) {
        Ok(at) => Ok(at < abandoned),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
/// When the bytes at `entry` last changed, epoch milliseconds.
fn changed_at(entry: &std::fs::DirEntry) -> std::io::Result<i64> {
    Ok(millis(entry.metadata()?.modified()?))
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
