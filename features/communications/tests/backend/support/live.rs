//! The stand-in for Routes' `live_route_data`, while `live` is on: the day it holds, its routes and
//! each driver's latest work phone as read live, as a test says.
use super::{DRIVERS, Known, routes::driven};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Feature, feature, optional, people::Source},
};
use dispatch_routes_contract::{DayPhones, Driven, DriverPhone, LiveRouteData, RouteDay};

pub(super) static LIVE: Feature = Feature {
    switch: optional("live", "Live", &[]),
    exposes: &[dispatch_core::expose!(
        dyn LiveRouteData,
        &STAND_IN,
        "live_route_data",
        "Today's routes as read live, as a test says.",
    )],
    ..feature("live")
};
struct Live;
static STAND_IN: Live = Live;
/// What it holds of each DSP.
static HELD: Known<Held> = Known::new();
/// The day it holds, its routes, and each driver's latest work phone.
#[derive(Default)]
struct Held {
    day: Option<String>,
    routes: Vec<RouteDay>,
    phones: Vec<DriverPhone>,
}

/// Has it answer `routes` as the live day's, as a live read leaves them, which Routes says nothing
/// of.
pub fn routed(dsp: &str, routes: Vec<RouteDay>) {
    HELD.of(dsp, |held| held.routes = routes);
}
/// Has it hold `day`, if any, with each driver's latest work phone `phones`, as a live read leaves
/// them, before Routes says so: the day it held before.
pub fn read_live(dsp: &str, day: Option<&str>, phones: Vec<DriverPhone>) -> Option<String> {
    HELD.of(dsp, |held| {
        held.phones = phones;
        std::mem::replace(&mut held.day, day.map(str::to_owned))
    })
}
/// Each driver's latest work phone it holds, while it holds a day.
pub(super) fn phones(dsp: &str) -> Vec<DriverPhone> {
    HELD.of(dsp, |held| {
        held.phones
            .iter()
            .filter(|_| held.day.is_some())
            .cloned()
            .collect()
    })
}

impl LiveRouteData for Live {
    fn source(&self) -> Source {
        DRIVERS
    }
    fn live_day(&self, _: &Store, dsp: &str) -> Result<Option<String>> {
        Ok(HELD.of(dsp, |held| held.day.clone()))
    }
    /// The live day's routes, while it holds one.
    fn day(&self, _: &Store, dsp: &str) -> Result<Driven> {
        Ok(driven(HELD.of(dsp, |held| {
            held.routes
                .iter()
                .filter(|_| held.day.is_some())
                .cloned()
                .collect()
        })))
    }
    /// The live day's routes driven by `ids`.
    fn routes(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Driven> {
        let mut day = LiveRouteData::day(self, db, dsp)?;
        day.routes.retain(|route| ids.contains(&route.driver_id));
        Ok(day)
    }
    /// Each driver's latest work phone of the live day; `Uncollected` without one.
    fn day_phones(&self, _: &Store, dsp: &str) -> Result<DayPhones> {
        Ok(match HELD.of(dsp, |held| held.day.is_some()) {
            true => DayPhones::Read(phones(dsp)),
            false => DayPhones::Uncollected,
        })
    }
}
