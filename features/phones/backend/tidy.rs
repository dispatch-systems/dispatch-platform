//! Phones' upkeep: letting go, hourly, of pictures added for a text and never sent within a day,
//! and of pairing codes expired.
use super::{
    files,
    storage::{DOMAIN, DeviceStore, FileStore},
};
use dispatch_core::{
    Error, Result, State,
    db::{self, Store},
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: pass,
};
/// How long a picture added for a text waits for it to be sent.
const UNSENT_MS: i64 = 24 * 60 * 60 * 1000;

fn failed(error: &Error) {
    observability::event("error", "phones_tidy_failed", json!({"error":error.code}));
}
fn pass(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let dsps = state
            .read(|db| {
                let mut on = Vec::new();
                for dsp in db.kept_dsps()? {
                    if db.feature_enabled(&dsp, "phones")? {
                        on.push(dsp);
                    }
                }
                Ok(on)
            })
            .await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        for dsp in dsps {
            let at = dsp.clone();
            let tidied = state
                .run_scoped(dsp, DOMAIN, move |db| {
                    let_go(db, &at, db::now() - UNSENT_MS)
                })
                .await;
            if let Err(error) = tidied {
                failed(&error);
            }
        }
    })
}
/// Lets go of the DSP's pictures added before `before` and never sent, and their bytes, each on
/// its own: bytes that can't be let go are logged, and the rest still go. Then of the codes
/// expired.
fn let_go(db: &Store, dsp: &str, before: i64) -> Result<()> {
    for sha256 in db.phones_tidy_files(dsp, before)? {
        if let Err(error) = files::forget(db, dsp, &sha256) {
            failed(&error);
        }
    }
    db.phones_tidy_pairings(dsp, db::now())
}

#[cfg(test)]
#[path = "../tests/backend/tidy.rs"]
mod tests;
