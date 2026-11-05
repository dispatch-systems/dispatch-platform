//! The days whose route data changed where nothing was at hand to tell of it: as the standard
//! collection makes a read the day's, and as a reprocess rebuilds one. Each is noted in the write
//! that keeps it (`route_days_untold`), then told to every feature that hears it
//! (`routes.data_changed`) once Routes hears its collection succeeded, or by the next minute's
//! upkeep, whichever is first, and let go once told: so a restart before then tells of it still,
//! and one cut short while telling tells of it again. A live read, a save and the extraction of a
//! day's work phones tell of it themselves, as they keep it.
use super::{COLLECTION, DOMAIN, RoutesStore};
use dispatch_core::{
    Error, Result, State,
    db::Db,
    foundation::observability,
    manifest::{
        Upkeep,
        events::{self, CollectionSucceeded},
    },
};
use dispatch_cortex::routes::JOB_KIND;
use dispatch_routes_contract::RouteDataChanged;
use rusqlite::params;
use serde_json::json;
use std::sync::Arc;

/// Notes, in the write that keeps it, that the DSP's route data of `day` changed, for the next
/// telling.
pub(super) fn note(db: &Db, day: &str) -> Result<()> {
    db.exec(
        "INSERT INTO route_days_untold (day,noted) VALUES (?,1) \
         ON CONFLICT(day) DO UPDATE SET noted=noted+1",
        [day],
    )?;
    Ok(())
}

/// Tells every feature that hears it of each of the DSP's days noted and not told of yet, oldest
/// first, while Routes is on for it, letting go of each once told, unless it was noted again
/// meanwhile. One whose reading or letting go fails is logged, and told by the next telling.
pub(super) async fn tell(state: &Arc<State>, dsp: &str) {
    let d = dsp.to_owned();
    let untold = state
        .read(move |db| {
            if !db.feature_enabled(&d, COLLECTION)? {
                return Ok(Vec::new());
            }
            db.routes_db(&d)?.query_as::<(String, i64)>(
                "SELECT day,noted FROM route_days_untold ORDER BY day",
                [],
            )
        })
        .await;
    let untold = match untold {
        Ok(untold) => untold,
        Err(error) => return failed(dsp, None, &error),
    };
    for (day, noted) in untold {
        events::publish(state, dsp, RouteDataChanged { day: day.clone() }).await;
        let (d, told) = (dsp.to_owned(), day.clone());
        let let_go = state
            .run_scoped(dsp, DOMAIN, move |db| {
                db.routes_db(&d)?.exec(
                    "DELETE FROM route_days_untold WHERE day=? AND noted=?",
                    params![told, noted],
                )
            })
            .await;
        if let Err(error) = let_go {
            failed(dsp, Some(&day), &error);
        }
    }
}

fn failed(dsp: &str, day: Option<&str>, error: &Error) {
    observability::event(
        "error",
        "routes_untold_failed",
        json!({"dspId": dsp, "day": day, "error": error.code}),
    );
}

/// Once a collection of routes succeeded for the DSP, tells of the days it changed.
pub fn collected(state: Arc<State>, dsp: String, event: CollectionSucceeded) -> Upkeep {
    Box::pin(async move {
        if event.keeps == JOB_KIND {
            tell(&state, &dsp).await;
        }
    })
}
