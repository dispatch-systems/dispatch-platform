//! Documents's upkeep. Every minute, sharing the folder with who joined the team and taking it
//! back from who left, and letting go of the copies of DSPs' Drives nobody has looked at for
//! half an hour. Hourly, noticing a Google connection that stopped working, so the page
//! says so and the owner can reconnect before someone finds out by trying, and checking the
//! sharing against Google's own list.
use super::{connection::broken, google::Google, mirror, storage::DocumentsStore, team};
use dispatch_core::{
    Error, State,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use futures_util::{StreamExt, stream};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: check,
};
pub const TEAM: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: follow,
};

/// Each pass checks DSPs in short database turns and shares at most two folders at
/// once. The scheduler supervises the pass; no detached syncs pile up between ticks.
fn follow(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        mirror::sweep();
        if Google::of(&state.config).is_err() {
            return;
        }
        let dsps = match state.read(|db| db.kept_dsps()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        stream::iter(dsps)
            .map(|dsp| {
                let state = state.clone();
                async move {
                    let selected = dsp.clone();
                    let changed = state
                        .read(move |db| {
                            Ok(db.feature_enabled(&selected, "documents")?
                                && db
                                    .documents_connection(&selected)?
                                    .is_some_and(|c| !c.broken)
                                && team::stale(db, &selected)?)
                        })
                        .await?;
                    if changed {
                        team::sync(&state, &dsp).await?;
                    }
                    dispatch_core::Result::Ok(())
                }
            })
            .buffer_unordered(2)
            .for_each(|result| async move {
                if let Err(error) = result {
                    failed(&error);
                }
            })
            .await;
    })
}

fn check(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let Ok(google) = Google::of(&state.config) else {
            return;
        };
        let dsps = match state.read(|db| db.kept_dsps()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        stream::iter(dsps)
            .map(|dsp| {
                let state = state.clone();
                let google = &google;
                async move {
                    let selected = dsp.clone();
                    let token = state
                        .read(move |db| {
                            if db.feature_enabled(&selected, "documents")?
                                && db
                                    .documents_connection(&selected)?
                                    .is_some_and(|c| !c.broken)
                            {
                                db.documents_refresh_token(&selected)
                            } else {
                                Ok(None)
                            }
                        })
                        .await?;
                    let Some(token) = token else {
                        return Ok(());
                    };
                    // Only Google's refusal breaks it: an outage leaves it for next hour.
                    match google.access(&token).await {
                        Err(error) if error.code == "google_connection_broken" => {
                            state.run(move |db| broken(db, &dsp)).await?;
                        }
                        Err(error) => return Err(error),
                        Ok(_) => {
                            team::sync(&state, &dsp).await?;
                        }
                    }
                    dispatch_core::Result::Ok(())
                }
            })
            .buffer_unordered(2)
            .for_each(|result| async move {
                if let Err(error) = result {
                    failed(&error);
                }
            })
            .await;
    })
}

fn failed(error: &Error) {
    observability::event(
        "error",
        "documents_check_failed",
        json!({"error": error.code}),
    );
}
