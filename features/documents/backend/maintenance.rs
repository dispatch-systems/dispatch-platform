//! Documents's upkeep, hourly: noticing a Google connection that stopped working, so the page
//! says so and the owner can reconnect before someone finds out by trying; and sharing the
//! folder with who joined the team, and taking it back from who left.
use super::{connection::broken, google::Google, storage::DocumentsStore, team};
use dispatch_core::{
    Error, State,
    foundation::observability,
    manifest::{Maintenance, Upkeep},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: check,
};

fn check(state: Arc<State>, due: bool) -> Upkeep {
    Box::pin(async move {
        if !due {
            return;
        }
        let Ok(google) = Google::of(&state.config) else {
            return;
        };
        let connected = state
            .read(|db| {
                let mut found = Vec::new();
                for dsp in db.kept_dsps()? {
                    if db.feature_enabled(&dsp, "documents")?
                        && db.documents_connection(&dsp)?.is_some_and(|c| !c.broken)
                        && let Some(token) = db.documents_refresh_token(&dsp)?
                    {
                        found.push((dsp, token));
                    }
                }
                Ok(found)
            })
            .await;
        let connected = match connected {
            Ok(connected) => connected,
            Err(error) => return failed(&error),
        };
        for (dsp, token) in connected {
            // Only Google's refusal breaks it: an outage leaves it as it is, for next hour.
            match google.access(&token).await {
                Err(error) if error.code == "google_connection_broken" => {
                    if let Err(error) = state.run(move |db| broken(db, &dsp)).await {
                        failed(&error);
                    }
                }
                Err(error) => failed(&error),
                // The team's sharing follows who joined, left or changed roles this hour.
                Ok(_) => {
                    if let Err(error) = team::sync(&state, &dsp).await {
                        failed(&error);
                    }
                }
            }
        }
    })
}

fn failed(error: &Error) {
    observability::event(
        "error",
        "documents_check_failed",
        json!({"error": error.code}),
    );
}
