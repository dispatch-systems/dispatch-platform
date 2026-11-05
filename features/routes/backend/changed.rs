//! The days whose route data changed where nothing was at hand to tell of it: as the standard
//! collection makes a read the day's, and as a reprocess rebuilds one. Each is noted in the write
//! that keeps it (`route_days_untold`, by an ID never given again), then told to every feature that
//! hears it (`routes.data_changed`) once Routes hears its collection succeeded, or by the next
//! minute's upkeep, whichever is first, and let go once told, the notes read alone, so one made
//! meanwhile is told again: a restart before then tells of it still, and one cut short while
//! telling tells of it again. A live read, a save and the extraction of a day's work phones tell of
//! it themselves, as they keep it.
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
/// telling, by the next ID (`route_days_noted`).
pub(super) fn note(db: &Db, day: &str) -> Result<()> {
    db.transaction(|| {
        db.exec("UPDATE route_days_noted SET last=last+1 WHERE id=1", [])?;
        db.exec(
            "INSERT INTO route_days_untold (id,day) SELECT last,? FROM route_days_noted WHERE id=1",
            [day],
        )?;
        Ok(())
    })
}
/// Each day noted and not told of yet, oldest first, with its latest note.
fn untold(db: &Db) -> Result<Vec<(String, i64)>> {
    db.query_as(
        "SELECT day,max(id) FROM route_days_untold GROUP BY day ORDER BY day",
        [],
    )
}
/// Lets go of `day`'s notes up to `last`, once told: one made since, by a later ID, as no ID is
/// given again, stays to be told.
fn told(db: &Db, day: &str, last: i64) -> Result<()> {
    db.exec(
        "DELETE FROM route_days_untold WHERE day=? AND id<=?",
        params![day, last],
    )?;
    Ok(())
}

/// Tells every feature that hears it of each of the DSP's days noted and not told of yet, oldest
/// first, while Routes is on for it, letting go of each once told, but a note made meanwhile. One
/// whose reading or letting go fails is logged, and told by the next telling.
pub(super) async fn tell(state: &Arc<State>, dsp: &str) {
    let d = dsp.to_owned();
    let untold = state
        .read(move |db| {
            if !db.feature_enabled(&d, COLLECTION)? {
                return Ok(Vec::new());
            }
            untold(&*db.routes_db(&d)?)
        })
        .await;
    let untold = match untold {
        Ok(untold) => untold,
        Err(error) => return failed(dsp, None, &error),
    };
    for (day, last) in untold {
        events::publish(state, dsp, RouteDataChanged { day: day.clone() }).await;
        let (d, told) = (dsp.to_owned(), day.clone());
        let let_go = state
            .run_scoped(dsp, DOMAIN, move |db| {
                self::told(&*db.routes_db(&d)?, &told, last)
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

#[cfg(test)]
#[path = "../tests/backend/changed.rs"]
mod tests;
