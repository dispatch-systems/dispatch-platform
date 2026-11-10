//! Text mutations and filing share one transaction in `changing`; conversation listeners wake
//! only after it commits. Placement failures keep texts pending for upkeep to retry.
//! Route-driven refiling preserves existing placements while the day's route data is unread.
//! The upkeep coordinator runs placement, refiling, live-day watching and phone cleanup in order.
mod maintenance;
mod placement;
mod reconciliation;

pub use maintenance::{expire, let_go_fetched, overdue, unfiled, watch_live_day};
pub use placement::belongs;
pub use reconciliation::{reconcile, refile_day, refiled};

use super::storage::TextStore;
use crate::backend::{live, phone::model::FETCHED_MS, storage::DOMAIN};
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use dispatch_routes_contract::RouteDataChanged;
use placement::place;
use reconciliation::{note, noted};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// Ten minutes, in milliseconds: how long a text waits to be filed, once placing it first failed,
/// before it's placed under its number (`overdue`).
pub const UNFILED_MS: i64 = 10 * 60 * 1000;

/// Every minute, each DSP's texts waiting to be filed, those waiting too long under their number,
/// the days whose filing again failed and today's filed again where they belong now; the live
/// day's end kept; its answers still waiting on an offline phone failed once they expire; and the
/// bytes of videos their phone sent an hour before let go of.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};

/// Makes the phone side's change `change` to the DSP's texts, and files the texts it kept or
/// changed, in one write and one transaction, so neither is kept without the other: a text that
/// can't be placed yet is kept waiting to be filed. Once both are committed, wakes the pages waiting
/// on a conversation that changed. `change` answers with what the caller is answered with, and the
/// IDs of the texts it kept or changed.
pub async fn changing<T: Send + 'static>(
    state: &Arc<State>,
    dsp: &str,
    change: impl FnOnce(&Store) -> Result<(T, Vec<String>)> + Send + 'static,
) -> Result<T> {
    let at = dsp.to_owned();
    let (answer, changed) = state
        .run_serving(dsp, DOMAIN, move |db| {
            db.dsp(&at)?.transaction(|| {
                let (answer, texts) = change(db)?;
                Ok((answer, file(db, &at, &texts)?))
            })
        })
        .await?;
    if changed {
        live::changed(state, dsp);
    }
    Ok(answer)
}

/// Routes says the DSP's route data of a day changed: notes the day to be filed again, then files
/// its texts again, which lets go of it, so one that fails, or that a restart cuts short, is filed
/// again by the upkeep.
pub fn refile(state: Arc<State>, dsp: String, changed: RouteDataChanged) -> Upkeep {
    Box::pin(async move {
        let of = || json!({"dsp": dsp, "day": changed.day});
        if let Err(error) = note(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error, of());
        }
        if let Err(error) = refile_day(&state, &dsp, &changed.day).await {
            failed("communications_texts_refile_failed", &error, of());
        }
    })
}

fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        let on = state
            .read(|db| crate::backend::switched_on(db, db.served_ids()?))
            .await;
        match on {
            Ok(dsps) => {
                for dsp in dsps {
                    let of = || json!({"dsp": dsp});
                    if let Err(error) = unfiled(&state, &dsp).await {
                        failed("communications_texts_failed", &error, of());
                    }
                    if let Err(error) = overdue(&state, &dsp, now()).await {
                        failed("communications_texts_failed", &error, of());
                    }
                    if let Err(error) = noted(&state, &dsp).await {
                        failed("communications_texts_refile_failed", &error, of());
                    }
                    if let Err(error) = reconcile(&state, &dsp).await {
                        failed("communications_texts_refile_failed", &error, of());
                    }
                    if let Err(error) = watch_live_day(&state, &dsp, now()).await {
                        failed("communications_texts_live_day_failed", &error, of());
                    }
                    if let Err(error) = expire(&state, &dsp, now()).await {
                        failed("communications_texts_expire_failed", &error, of());
                    }
                    if let Err(error) = let_go_fetched(&state, &dsp, now() - FETCHED_MS).await {
                        failed("communications_texts_fetched_failed", &error, of());
                    }
                }
            }
            Err(error) => failed("communications_texts_failed", &error, json!({})),
        }
    })
}

/// Files the texts `ids`, which the phone side kept or changed in this write: places each waiting
/// to be, then keeps whether the conversation each is filed in waits. Whether a conversation
/// changed.
fn file(db: &Store, dsp: &str, ids: &[String]) -> Result<bool> {
    if ids.is_empty() {
        return Ok(false);
    }
    let waiting = db.unfiled_communications_texts_of(dsp, ids)?;
    if !waiting.is_empty() {
        place(db, dsp, &waiting)?;
    }
    db.settle_communications_texts(dsp, ids)
}

/// Logs `event`, with `error`'s code beside what it was of, `of`: the DSP, and the text or day.
fn failed(event: &'static str, error: &Error, mut of: Value) {
    of["error"] = json!(error.code);
    observability::event("error", event, of);
}

#[cfg(test)]
#[path = "../../../tests/backend/texts/filing.rs"]
mod tests;
