//! The phone side's upkeep: letting go, hourly, of pictures added for a text and never sent within
//! a day, of bytes no file keeps or left arriving, and of pairing codes expired, each job on its
//! own, in each DSP with Communications on.
use super::{
    files,
    storage::{DeviceStore, FileStore},
};
use crate::backend::storage::DOMAIN;
use dispatch_core::{
    Error, State,
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

/// Reports what upkeep couldn't do, by the DSP it was for and the job that failed, when there were
/// any: tried again the next hour.
fn failed(dsp: Option<&str>, job: Option<&str>, error: &Error) {
    observability::event(
        "error",
        "phones_tidy_failed",
        json!({"dspId":dsp,"job":job,"error":error.code}),
    );
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
                    if db.feature_enabled(&dsp, crate::FEATURE.switch.id)? {
                        on.push(dsp);
                    }
                }
                Ok(on)
            })
            .await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => return failed(None, None, &error),
        };
        for dsp in dsps {
            let at = dsp.clone();
            let tidied = state
                .run_scoped(dsp.clone(), DOMAIN, move |db| {
                    let_go(db, &at, db::now() - UNSENT_MS);
                    Ok(())
                })
                .await;
            if let Err(error) = tidied {
                failed(Some(&dsp), None, &error);
            }
        }
    })
}
/// Lets go of the DSP's pictures added before `before` and never sent (`files`), then of the bytes
/// no file keeps or left arriving (`bytes`), then of the codes expired (`pairings`): each job on
/// its own, and each of the bytes, so one that fails is logged by its job and tried again next
/// time, and the rest still go.
fn let_go(db: &Store, dsp: &str, before: i64) {
    let job_failed = |job, error: &Error| failed(Some(dsp), Some(job), error);
    if let Err(error) = db.phones_tidy_files(dsp, before) {
        job_failed("files", &error);
    }
    for error in files::BYTES
        .reconcile(db, dsp)
        .unwrap_or_else(|error| vec![error])
    {
        job_failed("bytes", &error);
    }
    if let Err(error) = db.phones_tidy_pairings(dsp, db::now()) {
        job_failed("pairings", &error);
    }
}

#[cfg(test)]
#[path = "../../tests/backend/phone/tidy.rs"]
mod tests;
