//! Phones' upkeep: letting go, hourly, of pictures added for a text and never sent within a day,
//! and of pairing codes expired.
use super::{
    files,
    storage::{DOMAIN, PhonesStore},
};
use dispatch_core::{
    Error, State,
    db,
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
            let loose = state
                .run_scoped(dsp.clone(), DOMAIN, move |db| {
                    db.phones_tidy(&at, db::now() - UNSENT_MS)
                })
                .await;
            match loose {
                Ok(loose) => {
                    for sha256 in loose {
                        if let Err(error) = files::discard(&state, &dsp, &sha256).await {
                            failed(&error);
                        }
                    }
                }
                Err(error) => failed(&error),
            }
        }
    })
}
