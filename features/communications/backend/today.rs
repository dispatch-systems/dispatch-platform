//! Someone's route today, as the page shows it: its code, what the DSP calls its van (Vans'
//! `amazon_fleet`), when its wave starts, its stops and how many are done, and how it keeps up;
//! with the work phone they have now. From live route data while it holds today
//! (`live_route_data`), else as Routes has read today so far (`driver_routes`), with no work
//! phone. Of a driver's several itineraries, the one with the most stops, as a rescue they drove
//! too has fewer. Each feature is asked only while it's on for the DSP.
use super::texts::{live_day::LiveDay, numbers::work_number, zone};
use crate::api::types::Today;
use dispatch_core::{Result, db::Store, foundation::clock, manifest::people::Person, text_enum};
use dispatch_routes_contract::{self as routes, RouteDay};
use dispatch_vans_contract::{self as fleet, MOST_VINS};
use std::collections::{BTreeMap, BTreeSet, HashMap};

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/communications/api/generated/"))]
    /// How a route keeps up, as the page marks it: behind or at risk, as Amazon says, or done,
    /// every stop of it. None is marked on time.
    pub enum Pace {
        Behind => "behind",
        AtRisk => "at_risk",
        Done => "done",
    }
}
/// Amazon's words for a route behind, and at risk of finishing late.
const BEHIND: &str = "BEHIND";
const AT_RISK: &str = "AT_RISK";

/// How `route` keeps up.
fn pace(route: &RouteDay) -> Option<Pace> {
    if route.stops > 0 && route.stops_completed >= route.stops {
        return Some(Pace::Done);
    }
    match route.progress.as_deref() {
        Some(BEHIND) => Some(Pace::Behind),
        Some(AT_RISK) => Some(Pace::AtRisk),
        _ => None,
    }
}

/// The route of `driven` that's each `key`'s, as each driver's or each day's: of several, the one
/// with the most stops.
pub fn busiest<K: Ord>(
    driven: Vec<RouteDay>,
    key: impl Fn(&RouteDay) -> K,
) -> BTreeMap<K, RouteDay> {
    let mut busiest: BTreeMap<K, RouteDay> = BTreeMap::new();
    for route in driven {
        let of = key(&route);
        if busiest.get(&of).is_none_or(|kept| route.stops > kept.stops) {
            busiest.insert(of, route);
        }
    }
    busiest
}
/// Who drove a route.
pub fn driver(route: &RouteDay) -> String {
    route.driver_id.clone()
}

/// What the DSP calls each of the vans `vins`, by VIN, each asked once, as many at a time as Vans
/// reads: none while Vans is off.
pub fn vans<'a>(
    db: &Store,
    dsp: &str,
    vins: impl IntoIterator<Item = &'a String>,
) -> Result<HashMap<String, String>> {
    let vins: Vec<String> = vins
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .cloned()
        .collect();
    let Some(fleet) = fleet::for_dsp(db, dsp)?.filter(|_| !vins.is_empty()) else {
        return Ok(HashMap::new());
    };
    let mut named = HashMap::new();
    for read in vins.chunks(MOST_VINS) {
        named.extend(
            fleet
                .vans(db, dsp, read)?
                .into_iter()
                .filter_map(|van| Some((van.vin, van.name?))),
        );
    }
    Ok(named)
}

/// Someone's route today, `route`, in the van the DSP calls by `vans`, with the work phone
/// `phone`, its ten digits.
pub fn shown(
    route: Option<&RouteDay>,
    vans: &HashMap<String, String>,
    phone: Option<&str>,
) -> Today {
    Today {
        route: route.and_then(|route| route.route_code.clone()),
        van: route
            .and_then(|route| route.vin.as_ref())
            .and_then(|vin| vans.get(vin).cloned()),
        wave_at: route.and_then(|route| route.wave_start_at),
        stops: route.map(|route| route.stops).filter(|stops| *stops > 0),
        stops_done: route
            .filter(|route| route.stops > 0)
            .map(|route| route.stops_completed),
        pace: route.and_then(pace),
        phone: phone.map(work_number),
    }
}

/// The route `person` drives today, and the work phone they have now, if anything knows either.
pub fn of(db: &Store, dsp: &str, person: &Person) -> Result<Option<Today>> {
    if let Some(live) = LiveDay::read(db, dsp)? {
        let ids = ids(person.ids_of(live.reader.source()));
        return of_drivers(db, dsp, &live, &ids);
    }
    let Some(reader) = routes::for_dsp(db, dsp)? else {
        return Ok(None);
    };
    let ids = ids(person.ids_of(reader.source()));
    if ids.is_empty() {
        return Ok(None);
    }
    let day = clock::today(zone(db, dsp)?).to_string();
    let driven = busiest(reader.routes(db, dsp, &ids, &day, &day)?.routes, driver);
    let route = driven.values().max_by_key(|route| route.stops);
    let vans = vans(db, dsp, route.and_then(|route| route.vin.as_ref()))?;
    Ok(route.and_then(|route| known(shown(Some(route), &vans, None))))
}

/// The route the driver holding `ids` of the live day's source drives today, and the work phone
/// they have now, if they have either.
pub fn of_drivers(db: &Store, dsp: &str, live: &LiveDay, ids: &[String]) -> Result<Option<Today>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let driven = busiest(live.reader.routes(db, dsp, ids)?.routes, driver);
    let route = driven.values().max_by_key(|route| route.stops);
    let phone = live.phone_of(db, dsp, ids)?;
    let vans = vans(db, dsp, route.and_then(|route| route.vin.as_ref()))?;
    Ok(known(shown(
        route,
        &vans,
        phone.as_ref().map(|each| each.phone.as_str()),
    )))
}

/// `today`, while it says anything.
fn known(today: Today) -> Option<Today> {
    let known = today.route.is_some()
        || today.van.is_some()
        || today.wave_at.is_some()
        || today.stops.is_some()
        || today.phone.is_some();
    known.then_some(today)
}

/// The first of a person's `ids`, as many as one read of their routes names.
fn ids(ids: &[String]) -> Vec<String> {
    ids.iter().take(routes::MOST_IDS).cloned().collect()
}
