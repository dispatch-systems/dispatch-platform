//! The days whose route data changed where nothing was at hand to tell of it: as the standard
//! collection makes a read the day's, and as a reprocess rebuilds one. Each is noted as it's kept,
//! then told to every feature that hears it (`routes.data_changed`) once Routes hears its collection
//! succeeded, or by the next minute's upkeep, whichever is first. A live read, a save and the
//! extraction of a day's work phones tell of it themselves, as they keep it.
use dispatch_core::{
    State,
    manifest::{
        Upkeep,
        events::{self, CollectionSucceeded},
    },
};
use dispatch_cortex::routes::JOB_KIND;
use dispatch_routes_contract::RouteDataChanged;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, PoisonError},
};

/// The days not told of yet, by DSP. Gone with the process, so one a restart cuts short is told of
/// by nothing.
static UNTOLD: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());

/// Notes that the DSP's route data of `day` changed, once it's kept, for the next telling.
pub(super) fn note(dsp: &str, day: &str) {
    let mut untold = UNTOLD.lock().unwrap_or_else(PoisonError::into_inner);
    untold.insert((dsp.to_owned(), day.to_owned()));
}

/// Tells every feature that hears it of each of the DSP's days noted and not told of yet, oldest
/// first.
pub(super) async fn tell(state: &Arc<State>, dsp: &str) {
    let mut days = Vec::new();
    UNTOLD
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|(at, day)| {
            let theirs = at == dsp;
            if theirs {
                days.push(day.clone());
            }
            !theirs
        });
    for day in days {
        events::publish(state, dsp, RouteDataChanged { day }).await;
    }
}

/// Once a collection of routes succeeded for the DSP, tells of the days it changed.
pub fn collected(state: Arc<State>, dsp: String, event: CollectionSucceeded) -> Upkeep {
    Box::pin(async move {
        if event.keeps == JOB_KIND {
            tell(&state, &dsp).await;
        }
    })
}
