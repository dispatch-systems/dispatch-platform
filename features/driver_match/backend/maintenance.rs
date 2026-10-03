//! Driver Match's upkeep: giving every ID the DSP's collections hold a code, after each
//! collection and hourly.
use crate::backend::DriverMatchStore;
use dispatch_core::{
    Error, Result, State,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// Gives every ID each DSP's collections hold a Driver Match code, hourly. Collections give
/// new drivers their codes as they finish; this catches up anything they missed, and every
/// DSP's existing data on the first pass after startup.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: catch_up,
};

fn failed(event: &str, error: &Error) {
    observability::event("error", event, json!({"error":error.code}));
}

// Reading every collection takes the shared lock; only the writes take the platform lock,
// briefly.
async fn assign(state: &Arc<State>, dsp: String) -> Result<()> {
    let reading = dsp.clone();
    let found = state.read(move |db| db.driver_sources(&reading)).await?;
    let tenant = dsp.clone();
    state
        .run_scoped(dsp, super::DOMAIN, move |db| {
            db.assign_drivers(&tenant, found)
        })
        .await?;
    Ok(())
}

fn catch_up(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let dsps = state.read(|db| db.kept_dsps()).await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => return failed("driver_match_failed", &error),
        };
        for dsp in dsps {
            if let Err(error) = assign(&state, dsp).await {
                failed("driver_match_failed", &error);
            }
        }
    })
}

/// Whoever a collection brought in gets a Driver Match code. A failure here leaves the
/// collection as it is; the hourly pass catches the IDs up.
pub fn after_collection(state: Arc<State>, dsp: String) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = assign(&state, dsp.clone()).await {
            observability::event(
                "error",
                "driver_match.failed",
                json!({"dspId":dsp,"error":error.code}),
            );
        }
    })
}
