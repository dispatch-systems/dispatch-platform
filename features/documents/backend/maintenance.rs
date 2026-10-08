//! Documents's upkeep: noticing, hourly, a Google connection that stopped working, so the
//! page says so and the owner can reconnect before someone finds out by trying.
use super::{google::Google, storage::DocumentsStore};
use dispatch_core::{
    Error, Result, State,
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
                Ok(_) => {}
            }
        }
    })
}

fn broken(db: &dispatch_core::db::Store, dsp: &str) -> Result<()> {
    if db.break_documents_connection(dsp)? {
        db.audit(None, Some(dsp), "documents.connection_broken", "")?;
    }
    Ok(())
}

fn failed(error: &Error) {
    observability::event(
        "error",
        "documents_check_failed",
        json!({"error": error.code}),
    );
}
