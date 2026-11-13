//! Timecard's integration tests, one program for all of them, each file a module named for it:
//! one program links the crate and its collectors once, where a program per file linked them
//! five times.
mod meal_comparison;
mod meal_sync;
mod meals;
mod punches;
mod range_reads;

use dispatch_core::{
    Result,
    db::Store,
    manifest::{Feature, feature, optional, people::Source},
    testing,
};
use dispatch_routes_contract::{DayPhones, Driven, LiveRead, LiveRouteData};
use std::{collections::HashMap, sync::Mutex};

/// The stand-in for whatever keeps live route data: the read `LIVE` holds, for every DSP it is
/// switched on for.
struct Living;
/// The read the stand-in holds of today: where and when it was taken, the day's list, and each
/// itinerary by its id.
type Held = (LiveRead, String, HashMap<String, String>);
static LIVE: Mutex<Option<Held>> = Mutex::new(None);
static LIVING: Feature = Feature {
    switch: optional("living", "Living", &[]),
    exposes: &[dispatch_core::expose!(
        dyn LiveRouteData,
        &Living,
        "live_route_data",
        "The stand-in's live route data.",
    )],
    ..feature("living")
};
impl LiveRouteData for Living {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn live_day(&self, _: &Store, _: &str) -> Result<Option<String>> {
        Ok(LIVE.lock().unwrap().as_ref().map(|held| held.0.day.clone()))
    }
    fn day(&self, _: &Store, _: &str) -> Result<Driven> {
        Ok(Driven {
            days: Vec::new(),
            routes: Vec::new(),
        })
    }
    fn routes(&self, db: &Store, dsp: &str, _: &[String]) -> Result<Driven> {
        self.day(db, dsp)
    }
    fn day_phones(&self, _: &Store, _: &str) -> Result<DayPhones> {
        Ok(DayPhones::Uncollected)
    }
    fn live_read(&self, _: &Store, _: &str) -> Result<Option<LiveRead>> {
        Ok(LIVE.lock().unwrap().as_ref().map(|held| held.0.clone()))
    }
    fn live_response(&self, _: &Store, _: &str, itinerary: Option<&str>) -> Result<Option<String>> {
        let live = LIVE.lock().unwrap();
        Ok(live
            .as_ref()
            .and_then(|(_, list, itineraries)| match itinerary {
                Some(id) => itineraries.get(id).cloned(),
                None => Some(list.clone()),
            }))
    }
}

/// Installs Timecard, the collectors it keeps for, and the stand-in for live route data.
fn install() {
    testing::install(
        &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
        &[&dispatch_timecard::FEATURE, &LIVING],
    );
}
