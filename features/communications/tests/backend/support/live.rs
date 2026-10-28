//! The stand-in for Routes' `live_route_data`, while `live` is on: the day it holds, its routes as
//! read live and its holds on the work phones, as a test says.
use super::{DRIVERS, Known, routes::driven};
use dispatch_core::{
    Result,
    db::Store,
    manifest::{Feature, feature, optional, people::Source},
};
use dispatch_routes_contract::{
    DayPhones, Driven, LiveRouteData, RouteDay, WorkPhone, WorkPhoneHold,
};

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
/// The day it holds, its routes, and its holds on the work phones.
#[derive(Default)]
struct Held {
    day: Option<String>,
    routes: Vec<RouteDay>,
    holds: Vec<WorkPhoneHold>,
}

/// Has it answer `routes` as the live day's, as a live read leaves them, which Routes says nothing
/// of.
pub fn routed(dsp: &str, routes: Vec<RouteDay>) {
    HELD.of(dsp, |held| held.routes = routes);
}
/// Has it hold `day`, if any, with the holds `holds`: the day it held before.
pub(super) fn hold(dsp: &str, day: Option<&str>, holds: Vec<WorkPhoneHold>) -> Option<String> {
    HELD.of(dsp, |held| {
        held.holds = holds;
        std::mem::replace(&mut held.day, day.map(str::to_owned))
    })
}
/// The holds it has on the DSP's work phones.
pub(super) fn holds(dsp: &str) -> Vec<WorkPhoneHold> {
    HELD.of(dsp, |held| held.holds.clone())
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
    fn day_phones(&self, _: &Store, _: &str) -> Result<DayPhones> {
        unreachable!("Communications reads the live day's holds, not its phones")
    }
    /// The work phones the live day's holds that haven't ended hold.
    fn work_phones(&self, _: &Store, dsp: &str) -> Result<Vec<WorkPhone>> {
        Ok(holds(dsp)
            .into_iter()
            .filter(|hold| hold.until.is_none())
            .map(|hold| WorkPhone {
                driver_id: hold.driver_id,
                phone: hold.phone,
                since: hold.since,
            })
            .collect())
    }
    fn work_phone_holds(&self, _: &Store, dsp: &str) -> Result<Vec<WorkPhoneHold>> {
        Ok(holds(dsp))
    }
}
