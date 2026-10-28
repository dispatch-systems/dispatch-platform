//! The stand-in for Routes' `driver_routes`, while `driving` is on: drivers' routes and each saved
//! day's work phones, as a test says.
use super::{DRIVERS, Known, live};
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    db::Store,
    manifest::{Feature, feature, optional, people::Source},
};
use dispatch_routes_contract::{
    Collected, CollectedDay, DayPhones, Driven, DriverRoutes, MOST_DAYS, RouteDay, WorkPhoneHold,
};
use std::collections::{BTreeMap, BTreeSet};

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
/// Drivers' routes, and each saved day's work phones, by the day.
#[derive(Default)]
struct Kept {
    routes: Vec<RouteDay>,
    saved: BTreeMap<String, DayPhones>,
}

/// Has it answer that the DSP's drivers drove `routes`.
pub(super) fn know(dsp: &str, routes: Vec<RouteDay>) {
    KEPT.of(dsp, |kept| kept.routes = routes);
}
/// Has it keep `saved`, each a saved day and its work phones: any other day isn't collected.
pub(super) fn save(dsp: &str, saved: Vec<(String, DayPhones)>) {
    KEPT.of(dsp, |kept| kept.saved = saved.into_iter().collect());
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
    /// The work phones it keeps of a saved day; `Uncollected` for any other.
    fn day_phones(&self, _: &Store, dsp: &str, day: &str) -> Result<DayPhones> {
        let saved = KEPT.of(dsp, |kept| kept.saved.get(day).cloned());
        Ok(saved.unwrap_or(DayPhones::Uncollected))
    }
    /// Whether any driver's latest work phone it knows, live or saved, is `phone`.
    fn ever_work_phone(&self, _: &Store, dsp: &str, phone: &str) -> Result<bool> {
        let saved = KEPT.of(dsp, |kept| {
            kept.saved.values().any(|day| match day {
                DayPhones::Read(phones) => phones.iter().any(|each| each.phone == phone),
                DayPhones::Uncollected | DayPhones::Unextracted => false,
            })
        });
        Ok(saved || live::phones(dsp).iter().any(|each| each.phone == phone))
    }
    fn work_phone_holds(&self, _: &Store, _: &str, _: &str) -> Result<Vec<WorkPhoneHold>> {
        unreachable!("Communications reads each day's work phones, not holds")
    }
    fn was_work_phone(&self, _: &Store, _: &str, _: &str) -> Result<bool> {
        unreachable!("Communications asks whether a number was ever a work phone")
    }
}
