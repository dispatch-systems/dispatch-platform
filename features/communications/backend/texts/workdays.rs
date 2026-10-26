//! Each day a driver texted on a work phone, for the line that begins it in their work
//! conversation: the route and van they drove, today's as live route data holds it, an earlier
//! day's as Routes kept it (`driver_routes`), with what the DSP calls the van (`amazon_fleet`).
use super::{live_day::LiveDay, storage::Text, zone};
use crate::{api::types::Workday, backend::today};
use chrono::NaiveDate;
use dispatch_core::{Result, db::Store, foundation::clock};
use dispatch_routes_contract::{self as routes, RouteDay};
use std::collections::BTreeSet;

/// How many days one read of a driver's routes reaches, its first and last among them: Routes
/// reads a year at most.
const ROUTES_DAYS_MOST: i64 = 366;

/// The days `texts` were written on, each with the route and van the driver `id` of the source
/// `source` drove that day, as far as Routes knows.
pub fn of(db: &Store, dsp: &str, source: &str, id: &str, texts: &[Text]) -> Result<Vec<Workday>> {
    let zone = zone(db, dsp)?;
    let days: BTreeSet<NaiveDate> = texts.iter().map(|text| clock::day(zone, text.at)).collect();
    if days.is_empty() {
        return Ok(Vec::new());
    }
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
        for (from, to) in reaches(&days) {
            let kept = reader.routes(db, dsp, &ids, &from.to_string(), &to.to_string())?;
            driven.extend(
                kept.routes
                    .into_iter()
                    .filter(|route| live.as_ref().is_none_or(|live| route.day != live.day)),
            );
        }
    }
    // Only the days written on are shown, and their vans named.
    let shown: BTreeSet<String> = days.iter().map(NaiveDate::to_string).collect();
    driven.retain(|route| shown.contains(&route.day));
    let mut busiest = today::busiest(driven, |route| route.day.clone());
    let vans = today::vans(
        db,
        dsp,
        busiest.values().filter_map(|route| route.vin.as_ref()),
    )?;
    Ok(shown
        .into_iter()
        .map(|day| {
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

/// The reads of Routes that reach every one of `days`, in order: each from a day to the last
/// within its reach.
fn reaches(days: &BTreeSet<NaiveDate>) -> Vec<(NaiveDate, NaiveDate)> {
    let mut reaches: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    for &day in days {
        match reaches.last_mut() {
            Some((from, to)) if (day - *from).num_days() < ROUTES_DAYS_MOST => *to = day,
            _ => reaches.push((day, day)),
        }
    }
    reaches
}

#[cfg(test)]
#[path = "../../tests/backend/texts/workdays.rs"]
mod tests;
