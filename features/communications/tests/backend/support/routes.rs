//! The stand-in for Routes' `driver_routes`, while `driving` is on: drivers' routes and each saved
//! day's holds on the work phones, as a test says.
use super::{DRIVERS, Known, live};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    db::Store,
    manifest::{Feature, feature, optional, people::Source},
};
use dispatch_routes_contract::{
    Collected, CollectedDay, Driven, DriverRoutes, MOST_DAYS, RouteDay, WorkPhoneHold,
};
use std::collections::BTreeSet;

pub(super) static DRIVING: Feature = Feature {
    switch: optional("driving", "Driving", &[]),
    exposes: &[dispatch_core::expose!(
        dyn DriverRoutes,
        &STAND_IN,
        "driver_routes",
        "Drivers' routes, as a test says.",
    )],
    ..feature("driving")
};
struct Routing;
static STAND_IN: Routing = Routing;
/// What it keeps of each DSP.
static KEPT: Known<Kept> = Known::new();
/// Drivers' routes, and each saved day's holds.
#[derive(Default)]
struct Kept {
    routes: Vec<RouteDay>,
    saved: Vec<(String, WorkPhoneHold)>,
}

/// Has it answer that the DSP's drivers drove `routes`.
pub(super) fn know(dsp: &str, routes: Vec<RouteDay>) {
    KEPT.of(dsp, |kept| kept.routes = routes);
}
/// Has it keep `saved`, each a saved day's hold.
pub(super) fn save(dsp: &str, saved: Vec<(String, WorkPhoneHold)>) {
    KEPT.of(dsp, |kept| kept.saved = saved);
}
/// `routes`, with each day they're on read while under way.
pub(super) fn driven(routes: Vec<RouteDay>) -> Driven {
    let days: BTreeSet<String> = routes.iter().map(|route| route.day.clone()).collect();
    Driven {
        days: days
            .into_iter()
            .map(|day| CollectedDay {
                day,
                complete: false,
            })
            .collect(),
        routes,
    }
}

impl DriverRoutes for Routing {
    fn source(&self) -> Source {
        DRIVERS
    }
    fn collected(&self, _: &Store, _: &str) -> Result<Option<Collected>> {
        Ok(None)
    }
    /// The routes it knows driven by `ids` from `from` to `to`, and their days; refused across
    /// more days than Routes reads.
    fn routes(&self, _: &Store, dsp: &str, ids: &[String], from: &str, to: &str) -> Result<Driven> {
        let day = |day: &str| NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        if (day(to) - day(from)).num_days() >= MOST_DAYS {
            return Err(Error::new("invalid_input", 400));
        }
        let routes: Vec<RouteDay> = KEPT.of(dsp, |kept| {
            kept.routes
                .iter()
                .filter(|route| ids.contains(&route.driver_id))
                .filter(|route| (from..=to).contains(&route.day.as_str()))
                .cloned()
                .collect()
        });
        Ok(driven(routes))
    }
    /// Every route it knows driven on `day`.
    fn day(&self, _: &Store, dsp: &str, day: &str) -> Result<Driven> {
        let routes = KEPT.of(dsp, |kept| {
            kept.routes
                .iter()
                .filter(|route| route.day == day)
                .cloned()
                .collect()
        });
        Ok(driven(routes))
    }
    /// The holds it knows of a saved day.
    fn work_phone_holds(&self, _: &Store, dsp: &str, day: &str) -> Result<Vec<WorkPhoneHold>> {
        Ok(KEPT.of(dsp, |kept| {
            kept.saved
                .iter()
                .filter(|(saved, _)| saved == day)
                .map(|(_, hold)| hold.clone())
                .collect()
        }))
    }
    /// Whether any hold it knows, live or saved, is on `phone`.
    fn was_work_phone(&self, _: &Store, dsp: &str, phone: &str) -> Result<bool> {
        let saved = KEPT.of(dsp, |kept| {
            kept.saved.iter().any(|(_, hold)| hold.phone == phone)
        });
        Ok(saved || live::holds(dsp).iter().any(|hold| hold.phone == phone))
    }
}
