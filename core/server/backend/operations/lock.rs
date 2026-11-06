use crate::{Error, Result, db};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// The file every server and operation locks in the platform's root, left there once made.
pub(super) const LOCK: &str = ".platform.lock";
pub struct Lock {
    _file: File,
}
impl Lock {
    pub fn acquire(root: &Path) -> Result<Self> {
        db::private_dir(root)?;
        let path = root.join(LOCK);
        db::private_file(&path, true)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        file.try_lock_exclusive()
            .map_err(|_| Error::new("stop_services_before_operation", 409))?;
        Ok(Self { _file: file })
    }
}
