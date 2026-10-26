//! Each day a driver texted on a work phone, for the line that begins it in their work
//! conversation: the route and van they drove, today's as live route data holds it, an earlier
//! day's as Routes kept it (`driver_routes`), with what the DSP calls the van (`amazon_fleet`).
use super::{live_day::LiveDay, storage::Text, zone};
use crate::{api::types::Workday, backend::today};
use chrono::{Days, NaiveDate};
use dispatch_core::{Result, db::Store, foundation::clock};
use dispatch_routes_contract::{self as routes, RouteDay};
use std::collections::BTreeSet;

/// How many days one read of a driver's routes reaches: Routes reads a year at most.
const ROUTES_DAYS_MOST: u64 = 366;

/// The days `texts` were written on, each with the route and van the driver `id` of the source
/// `source` drove that day, as far as Routes knows.
pub fn of(db: &Store, dsp: &str, source: &str, id: &str, texts: &[Text]) -> Result<Vec<Workday>> {
    let zone = zone(db, dsp)?;
    let days: BTreeSet<NaiveDate> = texts.iter().map(|text| clock::day(zone, text.at)).collect();
    let (Some(&first), Some(&last)) = (days.first(), days.last()) else {
        return Ok(Vec::new());
    };
    let ids = [id.to_owned()];
    let mut driven: Vec<RouteDay> = Vec::new();
    // Today's as read live; the days before as Routes kept them.
    let live = LiveDay::read(db, dsp)?.filter(|live| live.source == source);
    if let Some(live) = &live {
        driven.extend(live.reader.routes(db, dsp, &ids)?.routes);
    }
    if let Some(reader) =
        routes::for_dsp(db, dsp)?.filter(|reader| reader.source().as_str() == source)
    {
        let reach = last
            .checked_sub_days(Days::new(ROUTES_DAYS_MOST - 1))
            .map_or(first, |reach| reach.max(first));
        let kept = reader.routes(db, dsp, &ids, &reach.to_string(), &last.to_string())?;
        driven.extend(
            kept.routes
                .into_iter()
                .filter(|route| live.as_ref().is_none_or(|live| route.day != live.day)),
        );
    }
    let mut busiest = today::busiest(driven, |route| route.day.clone());
    let vans = today::vans(
        db,
        dsp,
        busiest.values().filter_map(|route| route.vin.as_ref()),
    )?;
    Ok(days
        .into_iter()
        .map(|day| {
            let day = day.to_string();
            let route = busiest.remove(&day);
            let shown = today::shown(route.as_ref(), &vans, None);
            Workday {
                day,
                route: shown.route,
                van: shown.van,
            }
        })
        .collect())
}
