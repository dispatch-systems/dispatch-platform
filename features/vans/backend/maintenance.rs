//! Vans' upkeep: a DSP that has it gets a daily collection of the fleet, once, as soon as one
//! can run. The DSP changes or removes it as it likes; it never comes back.
use super::DOMAIN;
use crate::VansStore;
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

/// Whether the DSP's first schedule was made.
const SCHEDULED: &str = "schedule_made";
/// When the first schedule collects, where the DSP is: before the day's vans go out, and
/// apart from the roster's at 05:00, since a DSP's collections run one at a time.
const FIRST_RUN: &str = "04:30";

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(10 * 60),
    run: schedule,
};

/// Whether the DSP has Vans and never had its first schedule made.
fn unscheduled(db: &Store, dsp: &str) -> Result<bool> {
    Ok(db.feature_enabled(dsp, "vans")?
        && db.vans_db(dsp)?.setting(SCHEDULED, Value::Null)? != json!(true))
}
/// Makes the DSP's first schedule, if it still needs one and the fleet can be collected.
/// Answers whether it made one.
fn first_schedule(db: &Store, dsp: &str) -> Result<bool> {
    if !unscheduled(db, dsp)? {
        return Ok(false);
    }
    // Not yet connected, or no code to collect by: the next pass tries again.
    if db
        .save_schedule(
            dsp,
            None,
            &json!({"name":"Daily vans","collection":"vans","cadence":"daily",
                "localTime":FIRST_RUN,"enabled":true}),
        )
        .is_err()
    {
        return Ok(false);
    }
    db.vans_db(dsp)?.set(SCHEDULED, &json!(true))?;
    Ok(true)
}

fn failed(error: &Error) {
    observability::event("error", "vans.schedule_failed", json!({"error":error.code}));
}

fn schedule(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let dsps = match state.read(|db| db.kept_dsps()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        for dsp in dsps {
            // Read first: most passes find nothing to write.
            let id = dsp.clone();
            match state.read(move |db| unscheduled(db, &id)).await {
                Ok(true) => (),
                Ok(false) => continue,
                Err(error) => {
                    failed(&error);
                    continue;
                }
            }
            let id = dsp.clone();
            match state
                .run_scoped(dsp, DOMAIN, move |db| first_schedule(db, &id))
                .await
            {
                Ok(true) => {
                    state.schedule_revision.fetch_add(1, Ordering::Release);
                }
                Ok(false) => (),
                Err(error) => failed(&error),
            }
        }
    })
}

#[cfg(test)]
#[path = "../tests/backend/maintenance.rs"]
mod tests;
