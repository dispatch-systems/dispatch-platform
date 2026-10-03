//! Routes' upkeep: retiring route data past each DSP's retention window, and deleting what
//! no reader sees any more.
use crate::{
    Error, State,
    manifest::{Maintenance, Upkeep},
    observability,
    routedata::RoutesStore,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// Retires route data past each DSP's retention window, hourly, and deletes what no
/// reader sees any more in small steps, so the platform lock is never held for long.
pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60 * 60),
    run: clean,
};

fn failed(event: &str, error: &Error) {
    observability::event("error", event, json!({"error":error.code}));
}

fn clean(state: Arc<State>, expire: bool) -> Upkeep {
    Box::pin(async move {
        let dsps = state.read(|db| db.kept_dsps()).await;
        let dsps = match dsps {
            Ok(dsps) => dsps,
            Err(error) => return failed("routes_cleanup_failed", &error),
        };
        for dsp in dsps {
            if expire {
                let id = dsp.clone();
                if let Err(error) = state
                    .run_scoped(dsp.clone(), super::DOMAIN, move |db| db.expire_routes(&id))
                    .await
                {
                    failed("routes_expiry_failed", &error);
                }
            }
            // A day of rows is a few dozen steps; the rest waits for the next minute.
            let mut selected = None;
            for _ in 0..200 {
                let id = dsp.clone();
                match state
                    .run_bookkeeping(move |db| {
                        let more = super::sweep_routes_step(db, &id, &mut selected)?;
                        Ok((more, selected))
                    })
                    .await
                {
                    Ok((true, next)) => selected = next,
                    Ok((false, _)) => break,
                    Err(error) => {
                        failed("routes_cleanup_failed", &error);
                        break;
                    }
                }
            }
        }
    })
}
