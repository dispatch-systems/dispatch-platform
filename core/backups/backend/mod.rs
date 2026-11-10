//! Each DSP's backups: a schedule of its own, which the platform owner sets on the DSP's
//! Backups tab, and what it takes, kept in a bucket of their own (`config::BackupStore`), sealed
//! with a key its owner keeps off this server too. Each file is kept once by what it holds, so a
//! file that hasn't changed since the last backup isn't sent again, and each backup's
//! description is kept beside them, so a backup is read back with the bucket and the key alone.
//! This module alone owns the backups' SQL.
#[path = "../api/mod.rs"]
pub mod api;
pub mod contents;
pub mod download;
pub mod history;
pub mod keeper;
pub mod schedule;
pub mod seal;
mod take;
pub mod when;

pub use keeper::keeper;

use crate::{
    Error, Result,
    db::objects::Objects,
    foundation::{config::BackupStore, crypto},
};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// What the backups' bucket keeps each DSP's files as, and their descriptions as.
const FILES: &str = "backup_files";
const MANIFESTS: &str = "backup_manifests";

/// The backups' bucket and the key their files are sealed with.
pub struct Vault {
    pub objects: Objects,
    key: [u8; 32],
}
impl Vault {
    pub fn new(store: &BackupStore) -> Result<Self> {
        Ok(Self {
            objects: Objects::new(&store.objects)?,
            key: store.key,
        })
    }
    /// The name a file of the DSP's `generation` holding what has the SHA-256 `sha256` is kept
    /// by: keyed, so the bucket names no file by what it holds.
    fn file(&self, generation: &str, sha256: &str) -> String {
        self.named(&format!("file:{generation}:{sha256}"))
    }
    /// The name the description of the backup `id` is kept by.
    fn manifest(&self, id: &str) -> String {
        self.named(&format!("manifest:{id}"))
    }
    /// What the description of the DSP's backup `id` is sealed for: that backup alone, so one
    /// can't pass for another, nor a file for a description.
    fn described(dsp: &str, id: &str) -> String {
        format!("{dsp}\nmanifest\n{id}")
    }
    fn named(&self, value: &str) -> String {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&self.key)
            .expect("HMAC supports any key length");
        mac.update(value.as_bytes());
        crypto::hex(&mac.finalize().into_bytes())
    }
}

/// Runs `work` on a blocking thread, as a backup's files are copied, sealed and opened.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| Error::new("backup_failed", 500))?
}

#[cfg(test)]
#[path = "../tests/backend/backups.rs"]
mod tests;
