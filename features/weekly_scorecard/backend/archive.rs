//! Preserve the retired directory before deleting it, including when areas use different disks.
use dispatch_core::{
    Error, Result,
    db::{private_dir, private_file},
    ensure,
};
use std::{
    fs,
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::Path,
};

pub(super) fn check(source: &Path, target: &Path) -> Result<()> {
    if target.try_exists()? {
        ensure(
            matches(source, target)?,
            "weekly_scorecard_migration_backup_conflict",
            503,
        )?;
    }
    Ok(())
}

pub(super) fn finish(source: &Path, target: &Path) -> Result<()> {
    finish_with(source, target, |from, to| fs::rename(from, to))
}

pub(crate) fn finish_with(
    source: &Path,
    target: &Path,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    check(source, target)?;
    if target.try_exists()? {
        // A complete copy from a prior attempt can finish an interrupted deletion too.
        fs::remove_dir_all(source)?;
    } else {
        match rename(source, target) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
                let parent = target
                    .parent()
                    .ok_or_else(|| Error::new("unsafe_storage_path", 500))?;
                let stage = tempfile::Builder::new()
                    .prefix(".weekly-archive-")
                    .permissions(fs::Permissions::from_mode(0o700))
                    .tempdir_in(parent)?;
                copy(source, stage.path())?;
                ensure(
                    matches(source, stage.path())?,
                    "weekly_scorecard_migration_backup_conflict",
                    503,
                )?;
                fs::rename(stage.path(), target)?;
                fs::File::open(parent)?.sync_all()?;
                fs::remove_dir_all(source)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    for parent in [source.parent(), target.parent()].into_iter().flatten() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

fn copy(source: &Path, target: &Path) -> Result<()> {
    private_dir(source)?;
    private_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&from, &to)?;
        } else {
            private_file(&from, false)?;
            private_file(&to, true)?;
            fs::copy(&from, &to)?;
            fs::File::open(&to)?.sync_all()?;
        }
    }
    fs::File::open(target)?.sync_all()?;
    Ok(())
}

/// Every remaining source file must already be preserved. Archive-only files are retained.
fn matches(source: &Path, target: &Path) -> Result<bool> {
    if !target.is_dir() {
        return Ok(false);
    }
    private_dir(source)?;
    private_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            if !matches(&from, &to)? {
                return Ok(false);
            }
        } else {
            if !to.is_file() {
                return Ok(false);
            }
            private_file(&from, false)?;
            private_file(&to, false)?;
            let mut left = fs::File::open(&from)?;
            let mut right = fs::File::open(&to)?;
            let mut remaining = left.metadata()?.len();
            if remaining != right.metadata()?.len() {
                return Ok(false);
            }
            let (mut a, mut b) = ([0u8; 65536], [0u8; 65536]);
            while remaining > 0 {
                let take = remaining.min(a.len() as u64) as usize;
                left.read_exact(&mut a[..take])?;
                right.read_exact(&mut b[..take])?;
                if a[..take] != b[..take] {
                    return Ok(false);
                }
                remaining -= take as u64;
            }
        }
    }
    Ok(true)
}
