//! Driver Match's upkeep: giving every ID the DSP's collections hold a code, after each
//! collection and hourly, for every DSP.
use super::DriverMatchStore;
use dispatch_core::{
    Error, Result, State,
    foundation::observability,
    manifest::{
        Maintenance, Upkeep,
        events::{CollectionSucceeded, PeopleChanged},
    },
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

/// A pass that failed, for the DSP it was for when there was one: the next catches it up.
fn failed(dsp: Option<&str>, error: &Error) {
    observability::event(
        "error",
        "driver_match.failed",
        json!({"dspId":dsp,"error":error.code}),
    );
}

// Reading every collection takes the shared lock; only the writes take the platform lock,
// briefly.
async fn assign(state: &Arc<State>, dsp: String) -> Result<()> {
    let reading = dsp.clone();
    let found = state.read(move |db| db.read_pass(&reading)).await?;
    let tenant = dsp.clone();
    state
        .run_scoped(dsp, super::DOMAIN, move |db| db.write_pass(&tenant, found))
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
            Err(error) => return failed(None, &error),
        };
        for dsp in dsps {
            if let Err(error) = assign(&state, dsp.clone()).await {
                failed(Some(&dsp), &error);
            }
        }
    })
}

/// Whoever a collection brought in gets a Driver Match code. A failure here leaves the
/// collection as it is; the hourly pass catches the IDs up.
pub fn collected(state: Arc<State>, dsp: String, _: CollectionSucceeded) -> Upkeep {
    now(state, dsp)
}

/// Whoever a feature names outside a collection, as a workspace's new members, gets a code at
/// once rather than at the next collection.
pub fn changed(state: Arc<State>, dsp: String, _: PeopleChanged) -> Upkeep {
    now(state, dsp)
}

/// Gives every ID the DSP's features hold a code now; a failure waits for the hourly pass.
fn now(state: Arc<State>, dsp: String) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = assign(&state, dsp.clone()).await {
            failed(Some(&dsp), &error);
        }
    })
}
