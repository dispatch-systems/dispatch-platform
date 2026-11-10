//! Upkeep of where the pictures and files of both kinds are kept, hourly, in two tasks of their
//! own, so copying can't hold up the cache's limit. Copying (`MAINTENANCE`): in each DSP with
//! Communications on, for the phones' and Slack's stores of bytes each, what the object store has
//! that no file keeps let go of (`Folder::release`, once a day), then what waits in the DSP's data
//! copied to it (`Folder::copy`), `BATCH` at a time, the cache kept within its limit after each.
//! The cache (`CACHE`): the server's cached copies kept within its limit (`bytes::bound`),
//! `PICTURE_CACHE_MB` megabytes, 10 GB unless the environment says. Each on its own, so one that
//! fails is reported and tried again the next hour, and the rest still go.
use super::{
    bytes::{self, Folder},
    files, phone,
};
use dispatch_core::{
    Error, Result, State,
    foundation::{config::Config, observability},
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};
pub const CACHE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: bounding,
};
/// Every store of bytes.
const FOLDERS: [Folder; 2] = [phone::files::BYTES, files::BYTES];
/// How many megabytes of cached copies the server keeps, unless the environment says.
const CACHE_MB: u64 = 10 * 1024;
/// How many bytes are copied before the cache is kept within its limit again, as each may become
/// a cached copy.
const BATCH: usize = 100;

/// Reports what upkeep couldn't do, by the DSP and store it was for and the job that failed:
/// tried again the next hour.
fn failed(dsp: Option<&str>, owner: Option<&str>, job: &str, error: &Error) {
    observability::event(
        "error",
        "communications_copies_failed",
        json!({"dspId": dsp, "owner": owner, "job": job, "error": error.code}),
    );
}
fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due || state.objects.is_none() {
            return;
        }
        let dsps = state
            .read(|db| crate::backend::switched_on(db, db.kept_dsps()?))
            .await;
        match dsps {
            Ok(dsps) => {
                let most = most(&state.config);
                for dsp in dsps {
                    copied(&state, &dsp, most).await;
                }
            }
            Err(error) => failed(None, None, "dsps", &error),
        }
    })
}
fn bounding(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if due {
            bounded(&state, most(&state.config)).await;
        }
    })
}
/// For each of the DSP's stores of bytes: lets go of what the object store has that no file
/// keeps, then copies what waits in its data, `BATCH` at a time, keeping the server's cached
/// copies within `most` bytes after each.
async fn copied(state: &Arc<State>, dsp: &str, most: u64) {
    for folder in FOLDERS {
        for error in folder.release(state, dsp).await {
            failed(Some(dsp), Some(folder.owner), "release", &error);
        }
        let waiting = match folder.waiting(state, dsp).await {
            Ok(waiting) => waiting,
            Err(error) => {
                failed(Some(dsp), Some(folder.owner), "copy", &error);
                continue;
            }
        };
        for batch in waiting.chunks(BATCH) {
            for error in folder.copy(state, dsp, batch).await {
                failed(Some(dsp), Some(folder.owner), "copy", &error);
            }
            bounded(state, most).await;
        }
    }
}
/// Keeps the server's cached copies within `most` bytes.
async fn bounded(state: &Arc<State>, most: u64) {
    let root = state.config.root.clone();
    let bounded = tokio::task::spawn_blocking(move || {
        bytes::bound(&root, &FOLDERS.map(|folder| folder.owner), most)
    })
    .await
    .unwrap_or_else(|_| vec![Error::new("communications_cache_failed", 500)]);
    for error in bounded {
        failed(None, None, "cache", &error);
    }
}
/// How many bytes of cached copies the server keeps.
fn most(config: &Config) -> u64 {
    let mb = cache_mb(config).unwrap_or_else(|error| {
        failed(None, None, "cache", &error);
        CACHE_MB
    });
    mb * 1024 * 1024
}
/// How many megabytes of cached copies the server keeps: `PICTURE_CACHE_MB`, when the
/// environment says, else `CACHE_MB`.
fn cache_mb(config: &Config) -> Result<u64> {
    config
        .setting("PICTURE_CACHE_MB")
        .map_or(Ok(CACHE_MB), |mb| {
            mb.trim()
                .parse()
                .map_err(|_| Error::new("invalid_picture_cache", 500))
        })
}

#[cfg(test)]
#[path = "../tests/backend/copies.rs"]
mod tests;
