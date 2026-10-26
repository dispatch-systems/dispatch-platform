//! What Routes exposes to every other feature and agent tool: each driver's routes, day by day,
//! read from the rows its own pages read in historical route data; and today in live route data,
//! with the work phone each driver holds now; and who held which work phone, today and on a saved
//! day (`dispatch_routes_contract`).
use super::{RoutesStore, phones, routes_today};
use dispatch_core::{
    Result,
    db::{Db, Store, s},
    foundation::validate,
    manifest::people::Source,
};
use dispatch_routes_contract::{
    Collected, CollectedDay, Driven, DriverRoutes, LiveRouteData, MOST_IDS, RouteDay, Undelivered,
    WorkPhone, WorkPhoneHold,
};
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;

/// The most days one read covers.
const LONGEST: i64 = 366;

/// The days of a range with an active publication at the station, and whether each is final.
const DAYS: &str = "SELECT day,min(mode='final') complete FROM route_publications \
    WHERE station=?1 AND active=1 AND day BETWEEN ?2 AND ?3 GROUP BY day ORDER BY day";
/// Each query below twice: for every driver (?4 is null), and for the IDs in ?4, which finds
/// them by driver first; one query for both would read every driver's rows in the range.
macro_rules! by_driver {
    ($sql:literal, $column:literal, $order:literal) => {
        [
            concat!($sql, " AND ?4 IS NULL ", $order),
            concat!(
                $sql,
                " AND ",
                $column,
                " IN (SELECT value FROM json_each(?4)) ",
                $order
            ),
        ]
    };
}
/// The drivers' itineraries over a range, with their day summaries and removed packages: those
/// of the IDs given, or every driver's. The removed are counted in `tasks_removed`, which holds
/// only those, so `active=0` stays as it has it: any other term reads every task of the days.
const ROUTES: [&str; 2] = by_driver!(
    "SELECT d.publication_id,d.itinerary_id,d.day,d.transporter_id,i.route_code,\
    i.execution_status,i.progress_status,i.vin,i.wave_start_at,i.planned_departure_at,d.departed_at,\
    d.first_stop_at,d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.delivered,\
    d.not_delivered,d.breaks_secs,d.overtime_secs,(SELECT count(*) FROM tasks t \
    WHERE t.publication_id=d.publication_id AND t.itinerary_id=d.itinerary_id AND t.active=0) removed \
    FROM driver_days d JOIN route_publications p ON p.id=d.publication_id AND p.active=1 AND p.station=?1 \
    JOIN itineraries i ON i.publication_id=d.publication_id AND i.itinerary_id=d.itinerary_id \
    WHERE d.day BETWEEN ?2 AND ?3",
    "d.transporter_id",
    "ORDER BY d.day,i.route_code,d.itinerary_id"
);
/// The provider's routes over a range each driver is assigned to and has no itinerary on yet, as
/// before dispatch: those of the IDs given, or every driver's. The route list may name other
/// providers' routes at the station.
const ASSIGNED: [&str; 2] = by_driver!(
    "SELECT r.day,a.value transporter_id,r.route_code,r.route_status,r.progress_status,\
    r.planned_departure_at,r.total_stops FROM routes r \
    JOIN route_publications p ON p.id=r.publication_id AND p.active=1 AND p.station=?1 \
    JOIN json_each(r.transporter_ids) a \
    WHERE r.day BETWEEN ?2 AND ?3 AND r.company_id=p.provider \
    AND NOT EXISTS (SELECT 1 FROM driver_days d WHERE d.publication_id=r.publication_id \
    AND d.transporter_id=a.value AND d.route_code IS r.route_code)",
    "a.value",
    "ORDER BY r.day,r.route_code"
);
/// The drivers' drop-offs over a range that weren't delivered, as the day summaries count them:
/// those of the IDs given, or every driver's. A driver's are found in `tasks_undelivered`, which
/// holds only those, so the terms stay as it has them: any others read every task of the days.
const UNDELIVERED: [&str; 2] = by_driver!(
    "SELECT t.publication_id,t.itinerary_id,t.tracking_id,t.task_state,\
    t.state_context,t.executed_at FROM tasks t \
    JOIN route_publications p ON p.id=t.publication_id AND p.active=1 AND p.station=?1 \
    WHERE t.day BETWEEN ?2 AND ?3 \
    AND t.active=1 AND t.task_type='DROP_OFF' AND t.task_state IS NOT 'DELIVERED'",
    "t.transporter_id",
    "ORDER BY t.executed_at,t.task_id"
);

/// Each driver's routes in historical route data, as `DriverRoutes` reads them.
pub(crate) struct Exposed;
pub(crate) static EXPOSED: Exposed = Exposed;
/// Today in live route data, as `LiveRouteData` reads it.
pub(crate) struct Live;
pub(crate) static LIVE: Live = Live;

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
                from: text(&row, "first")?,
                through: text(&row, "last")?,
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
        validate::days(from, to, LONGEST)?;
        validate::ids(ids, MOST_IDS)?;
        let station = db.profile(dsp)?.station_code;
        driven(&*db.routes_db(dsp)?, &station, Some(ids), from, to, false)
    }
    fn day(&self, db: &Store, dsp: &str, day: &str) -> Result<Driven> {
        validate::days(day, day, 1)?;
        let station = db.profile(dsp)?.station_code;
        driven(&*db.routes_db(dsp)?, &station, None, day, day, false)
    }
    fn work_phone_holds(&self, db: &Store, dsp: &str, day: &str) -> Result<Vec<WorkPhoneHold>> {
        validate::days(day, day, 1)?;
        holds(&*db.routes_db(dsp)?, day)
    }
    fn was_work_phone(&self, db: &Store, dsp: &str, phone: &str) -> Result<bool> {
        let Some(phone) = phones::digits(phone) else {
            return Ok(false);
        };
        Ok(phones::ever_held(&*db.routes_live_db(dsp)?, &phone)?
            || phones::ever_held(&*db.routes_db(dsp)?, &phone)?)
    }
}

impl LiveRouteData for Live {
    fn source(&self) -> Source {
        dispatch_cortex::SOURCE
    }
    fn live_day(&self, db: &Store, dsp: &str) -> Result<Option<String>> {
        let station = db.profile(dsp)?.station_code;
        let today = routes_today(db, dsp)?.to_string();
        let held = db.routes_live_db(dsp)?.one(
            "SELECT 1 FROM route_publications WHERE station=? AND day=? AND active=1",
            [&station, &today],
        )?;
        Ok(held.map(|_| today))
    }
    fn day(&self, db: &Store, dsp: &str) -> Result<Driven> {
        self.driven(db, dsp, None)
    }
    fn routes(&self, db: &Store, dsp: &str, ids: &[String]) -> Result<Driven> {
        validate::ids(ids, MOST_IDS)?;
        self.driven(db, dsp, Some(ids))
    }
    fn work_phones(&self, db: &Store, dsp: &str) -> Result<Vec<WorkPhone>> {
        let Some(day) = self.live_day(db, dsp)? else {
            return Ok(Vec::new());
        };
        let held = db.routes_live_db(dsp)?.all(
            "SELECT transporter_id,phone,since FROM work_phones WHERE day=? AND until IS NULL \
             ORDER BY transporter_id",
            [&day],
        )?;
        Ok(held
            .iter()
            .map(|hold| WorkPhone {
                driver_id: s(hold, "transporter_id").into(),
                phone: s(hold, "phone").into(),
                since: hold["since"].as_i64().unwrap_or(0),
            })
            .collect())
    }
    fn work_phone_holds(&self, db: &Store, dsp: &str) -> Result<Vec<WorkPhoneHold>> {
        let Some(day) = self.live_day(db, dsp)? else {
            return Ok(Vec::new());
        };
        holds(&*db.routes_live_db(dsp)?, &day)
    }
}
impl Live {
    /// What the drivers holding `ids`, or every driver, drive today as live route data holds it,
    /// the routes they're assigned to with it; nothing when it holds no read of today.
    fn driven(&self, db: &Store, dsp: &str, ids: Option<&[String]>) -> Result<Driven> {
        let Some(day) = self.live_day(db, dsp)? else {
            return Ok(Driven {
                days: Vec::new(),
                routes: Vec::new(),
            });
        };
        let station = db.profile(dsp)?.station_code;
        driven(&*db.routes_live_db(dsp)?, &station, ids, &day, &day, true)
    }
}

/// What the drivers holding `ids`, or every driver, drove from `from` to `to` at `station` as
/// `routes` holds it, and which of those days were collected; with the routes they're assigned
/// to and haven't started when `assigned`.
fn driven(
    routes: &Db,
    station: &str,
    ids: Option<&[String]>,
    from: &str,
    to: &str,
    assigned: bool,
) -> Result<Driven> {
    let by = usize::from(ids.is_some());
    let ids = ids.map(serde_json::to_string).transpose()?;
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
        for row in routes.all(ROUTES[by], params![station, from, to, ids])? {
            found.insert(itinerary(&row), driven.len());
            driven.push(route_day(&row));
        }
        for row in routes.all(UNDELIVERED[by], params![station, from, to, ids])? {
            if let Some(&at) = found.get(&itinerary(&row)) {
                driven[at].undelivered.push(undelivered(&row));
            }
        }
        if assigned {
            let planned = routes.all(ASSIGNED[by], params![station, from, to, ids])?;
            driven.extend(planned.iter().map(planned_day));
            driven.sort_by(|a, b| (&a.day, &a.route_code).cmp(&(&b.day, &b.route_code)));
        }
        Ok(Driven {
            days,
            routes: driven,
        })
    })
}

/// Every hold on a work phone `routes` keeps of `day`, oldest first, as the contract has them.
fn holds(routes: &Db, day: &str) -> Result<Vec<WorkPhoneHold>> {
    let holds = phones::holds(routes, day)?.into_iter();
    Ok(holds
        .map(|hold| WorkPhoneHold {
            driver_id: hold.transporter_id,
            phone: hold.phone,
            since: hold.since,
            until: hold.until,
        })
        .collect())
}

/// A row's text, when it has some.
fn text(row: &Value, key: &str) -> Option<String> {
    row[key].as_str().map(str::to_owned)
}
/// The publication and itinerary a row belongs to.
fn itinerary(row: &Value) -> (String, String) {
    (
        s(row, "publication_id").to_owned(),
        s(row, "itinerary_id").to_owned(),
    )
}
fn route_day(row: &Value) -> RouteDay {
    let count = |key: &str| row[key].as_i64().unwrap_or(0);
    RouteDay {
        day: s(row, "day").into(),
        driver_id: s(row, "transporter_id").into(),
        route_code: text(row, "route_code"),
        status: text(row, "execution_status"),
        progress: text(row, "progress_status"),
        vin: text(row, "vin"),
        wave_start_at: row["wave_start_at"].as_i64(),
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
/// A route a driver is assigned to and hasn't started: as planned, nothing done yet.
fn planned_day(row: &Value) -> RouteDay {
    RouteDay {
        day: s(row, "day").into(),
        driver_id: s(row, "transporter_id").into(),
        route_code: text(row, "route_code"),
        status: text(row, "route_status"),
        progress: text(row, "progress_status"),
        vin: None,
        wave_start_at: None,
        planned_departure_at: row["planned_departure_at"].as_i64(),
        departed_at: None,
        first_delivery_at: None,
        last_delivery_at: None,
        session_end_at: None,
        stops: row["total_stops"].as_i64().unwrap_or(0),
        stops_completed: 0,
        delivered: 0,
        not_delivered: 0,
        removed: 0,
        breaks_secs: None,
        overtime_secs: None,
        undelivered: Vec::new(),
    }
}
fn undelivered(row: &Value) -> Undelivered {
    Undelivered {
        tracking_id: text(row, "tracking_id"),
        state: text(row, "task_state"),
        // Amazon writes NONE where it gives no reason.
        reason: text(row, "state_context").filter(|reason| reason != "NONE"),
        attempted_at: row["executed_at"].as_i64(),
    }
}

#[cfg(test)]
#[path = "../tests/backend/exposed.rs"]
mod tests;
