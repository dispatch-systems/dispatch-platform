use crate::{Result, ensure, foundation::crypto};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};
/// Folders nothing is made in again while the server runs, as a deleted DSP's: what was under
/// way as it went, holding where it writes, would make it again otherwise.
static RETIRED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
fn retired() -> MutexGuard<'static, Vec<PathBuf>> {
    RETIRED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
/// Makes nothing in `folder` again from now on (`private_dir`): before it's moved away or
/// deleted, so what's made in it meanwhile goes with it.
pub fn retire(folder: &Path) {
    retired().push(folder.into());
}
/// The private folder `path`, made with each folder it's in that isn't there yet, except in a
/// retired one (`retire`).
pub fn private_dir(path: &Path) -> Result<PathBuf> {
    ensure(path.is_absolute(), "unsafe_storage_path", 500)?;
    let mut cursor = PathBuf::from("/");
    for part in path.components().skip(1) {
        ensure(
            matches!(part, std::path::Component::Normal(_)),
            "unsafe_storage_path",
            500,
        )?;
        cursor.push(part);
        if !cursor.try_exists()? {
            // Made while no folder it's in is retired, as retiring waits its turn.
            let retired = retired();
            ensure(
                !retired.iter().any(|folder| cursor.starts_with(folder)),
                "dsp_not_found",
                404,
            )?;
            // Made by another meanwhile is as good: what's there is checked all the same.
            if let Err(error) = fs::DirBuilder::new().mode(0o700).create(&cursor)
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(error.into());
            }
        }
        let stat = fs::symlink_metadata(&cursor)?;
        ensure(
            stat.is_dir() && !stat.file_type().is_symlink(),
            "unsafe_storage_path",
            500,
        )?;
    }
    let stat = fs::metadata(path)?;
    ensure(
        stat.uid() == unsafe { libc::geteuid() } && stat.mode() & 0o077 == 0,
        "private_storage_permissions_required",
        500,
    )?;
    Ok(path.into())
}
pub fn private_file(path: &Path, create: bool) -> Result<()> {
    private_dir(
        path.parent()
            .ok_or_else(|| crate::Error::new("unsafe_storage_path", 500))?,
    )?;
    if create {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    check_metadata(path, fs::symlink_metadata(path))
}
fn check_metadata(path: &Path, metadata: std::io::Result<fs::Metadata>) -> Result<()> {
    match metadata {
        // SQLite removes its -wal, -shm and -journal files when a connection closes.
        // An lstat racing that unlink can still succeed and report no links; the
        // file is already gone, which is the same as not found.
        Ok(s) if s.nlink() == 0 => Ok(()),
        Ok(s) => {
            let safe = s.is_file()
                && !s.file_type().is_symlink()
                && s.nlink() == 1
                && s.uid() == unsafe { libc::geteuid() }
                && s.mode() & 0o077 == 0;
            if !safe {
                crate::foundation::observability::event(
                    "error",
                    "storage.file_rejected",
                    json!({
                        "links":s.nlink(), "mode":s.mode() & 0o777,
                        "ownerMatches":s.uid() == unsafe { libc::geteuid() },
                        "regular":s.is_file(), "symlink":s.file_type().is_symlink(),
                        "sqliteSidecar":path.to_string_lossy().ends_with("-wal")
                            || path.to_string_lossy().ends_with("-shm")
                            || path.to_string_lossy().ends_with("-journal")
                    }),
                );
            }
            ensure(safe, "unsafe_storage_file", 500)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
/// The lock file `path`, made private when it isn't there yet and opened for its caller to
/// lock, never through a link.
pub fn lock_file(path: &Path) -> Result<fs::File> {
    private_file(path, true)?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?)
}
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    private_file(path, false)?;
    let temp = path.with_extension(crypto::id("tmp")?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn key_file(path: &Path) -> Result<Vec<u8>> {
    private_file(path, false)?;
    if !path.try_exists()? {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(&crypto::random::<32>()?)?;
        f.sync_all()?;
    }
    let key = fs::read(path)?;
    ensure(key.len() == 32, "invalid_key", 500)?;
    Ok(key)
}

#[cfg(test)]
#[path = "../tests/backend/files.rs"]
mod tests;
