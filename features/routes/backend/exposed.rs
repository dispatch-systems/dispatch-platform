//! What Routes exposes to every other feature and agent tool: each driver's routes, day by day,
//! read from the rows its own pages read (`dispatch_routes_contract`).
use super::RoutesStore;
use chrono::NaiveDate;
use dispatch_core::{
    Error, Result,
    db::{Store, s},
    ensure,
    foundation::validate,
    manifest::people::Source,
};
use dispatch_routes_contract::{
    Collected, CollectedDay, Driven, DriverRoutes, RouteDay, Undelivered,
};
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;

/// The most days one read covers.
const LONGEST: i64 = 366;
/// The most IDs one read names, and the longest ID.
const MOST_IDS: usize = 16;
const LONGEST_ID: usize = 128;

/// The days of a range with an active publication at the station, and whether each is final.
const DAYS: &str = "SELECT day,min(mode='final') complete FROM route_publications \
    WHERE station=?1 AND active=1 AND day BETWEEN ?2 AND ?3 GROUP BY day ORDER BY day";
/// The drivers' itineraries over a range, with their day summaries and removed packages.
const ROUTES: &str = "SELECT d.publication_id,d.itinerary_id,d.day,d.transporter_id,i.route_code,\
    i.execution_status,i.progress_status,i.vin,i.planned_departure_at,d.departed_at,d.first_stop_at,\
    d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.delivered,d.not_delivered,\
    d.breaks_secs,d.overtime_secs,(SELECT count(*) FROM tasks t WHERE t.publication_id=d.publication_id \
    AND t.itinerary_id=d.itinerary_id AND t.active=0) removed \
    FROM driver_days d JOIN route_publications p ON p.id=d.publication_id AND p.active=1 AND p.station=?1 \
    JOIN itineraries i ON i.publication_id=d.publication_id AND i.itinerary_id=d.itinerary_id \
    WHERE d.day BETWEEN ?2 AND ?3 AND d.transporter_id IN (SELECT value FROM json_each(?4)) \
    ORDER BY d.day,i.route_code,d.itinerary_id";
/// The drivers' drop-offs over a range that weren't delivered, as the day summaries count them.
const UNDELIVERED: &str = "SELECT t.publication_id,t.itinerary_id,t.tracking_id,t.task_state,\
    t.state_context,t.executed_at FROM tasks t \
    JOIN route_publications p ON p.id=t.publication_id AND p.active=1 AND p.station=?1 \
    WHERE t.day BETWEEN ?2 AND ?3 AND t.transporter_id IN (SELECT value FROM json_each(?4)) \
    AND t.active=1 AND t.task_type='DROP_OFF' AND t.task_state IS NOT 'DELIVERED' \
    ORDER BY t.executed_at,t.task_id";

/// Each driver's routes, as `DriverRoutes` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;

impl DriverRoutes for Exposed {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>> {
        let station = db.profile(dsp)?.station_code;
        let row = db.routes_db(dsp)?.one(
            "SELECT min(day) first,max(day) last FROM route_publications WHERE station=? AND active=1",
            [&station],
        )?;
        Ok(row.and_then(|row| {
            Some(Collected {
                from: row["first"].as_str()?.to_owned(),
                through: row["last"].as_str()?.to_owned(),
            })
        }))
    }
    fn routes(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Driven> {
        validate::date(from)?;
        validate::date(to)?;
        let date = |day: &str| {
            NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))
        };
        ensure(
            from <= to
                && (date(to)? - date(from)?).num_days() < LONGEST
                && ids.len() <= MOST_IDS
                && ids
                    .iter()
                    .all(|id| !id.is_empty() && id.chars().count() <= LONGEST_ID),
            "invalid_input",
            400,
        )?;
        let station = db.profile(dsp)?.station_code;
        let ids = serde_json::to_string(ids)?;
        let routes = db.routes_db(dsp)?;
        // One read, so a recollection can't land between the itineraries and their packages.
        routes.transaction(|| {
            let days = routes
                .all(DAYS, params![station, from, to])?
                .iter()
                .map(|row| CollectedDay {
                    day: s(row, "day").into(),
                    complete: row["complete"] == 1,
                })
                .collect();
            let mut driven = Vec::new();
            let mut found = HashMap::new();
            for row in routes.all(ROUTES, params![station, from, to, ids])? {
                found.insert(itinerary(&row), driven.len());
                driven.push(route_day(&row));
            }
            for row in routes.all(UNDELIVERED, params![station, from, to, ids])? {
                if let Some(&at) = found.get(&itinerary(&row)) {
                    driven[at].undelivered.push(Undelivered {
                        tracking_id: row["tracking_id"].as_str().map(str::to_owned),
                        state: row["task_state"].as_str().map(str::to_owned),
                        // Amazon writes NONE where it gives no reason.
                        reason: row["state_context"]
                            .as_str()
                            .filter(|reason| *reason != "NONE")
                            .map(str::to_owned),
                        attempted_at: row["executed_at"].as_i64(),
                    });
                }
            }
            Ok(Driven {
                days,
                routes: driven,
            })
        })
    }
}

/// The publication and itinerary a row belongs to.
fn itinerary(row: &Value) -> (String, String) {
    (
        s(row, "publication_id").to_owned(),
        s(row, "itinerary_id").to_owned(),
    )
}
fn route_day(row: &Value) -> RouteDay {
    let text = |key: &str| row[key].as_str().map(str::to_owned);
    let count = |key: &str| row[key].as_i64().unwrap_or(0);
    RouteDay {
        day: s(row, "day").into(),
        driver_id: s(row, "transporter_id").into(),
        route_code: text("route_code"),
        status: text("execution_status"),
        progress: text("progress_status"),
        vin: text("vin"),
        planned_departure_at: row["planned_departure_at"].as_i64(),
        departed_at: row["departed_at"].as_i64(),
        first_delivery_at: row["first_stop_at"].as_i64(),
        last_delivery_at: row["last_stop_at"].as_i64(),
        session_end_at: row["session_end_at"].as_i64(),
        stops: count("stops_total"),
        stops_completed: count("stops_completed"),
        delivered: count("delivered"),
        not_delivered: count("not_delivered"),
        removed: count("removed"),
        breaks_secs: row["breaks_secs"].as_i64(),
        overtime_secs: row["overtime_secs"].as_i64(),
        undelivered: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
