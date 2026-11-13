//! Dashboard and package queries with their row-to-API adapters.
use super::{RoutesStore, stored_itinerary};
use crate::backend::shaping::Shaper;
use crate::{
    api::types::{
        RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
        RoutePackage, RoutePackageEvent, RoutePublication, RouteStop, RouteTask, RouteUnknownStop,
    },
    backend::scope::{default_day, routes_today},
};
use dispatch_core::{
    Result,
    db::{Db, Store, s},
};
use dispatch_cortex::routes::Mode;
use rusqlite::params;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Every day with a publication at the DSP's station, newest first.
pub(super) fn route_days(store: &Store, id: &str) -> Result<RouteDays> {
    let today = routes_today(store, id)?;
    let station = store.profile(id)?.station_code;
    let db = store.routes_db(id)?;
    let days = db
        .all(
            &format!("{PUBLICATION_SELECT} WHERE station=? AND active=1 ORDER BY day DESC"),
            [&station],
        )?
        .iter()
        .map(publication)
        .collect();
    Ok(RouteDays {
        station,
        latest_day: default_day(Mode::Final, today),
        days,
    })
}

/// A day's publication with its driver summaries, or none.
pub(super) fn route_day(store: &Store, id: &str, day: &str) -> Result<Option<RouteDayView>> {
    dispatch_core::foundation::validate::date(day)?;
    let station = store.profile(id)?.station_code;
    let db = store.routes_db(id)?;
    let Some(row) = db.one(
        &format!("{PUBLICATION_SELECT} WHERE station=? AND day=? AND active=1"),
        [&station, day],
    )?
    else {
        return Ok(None);
    };
    let itineraries = db
        .all(
            &format!(
                "{ITINERARY_SELECT} WHERE i.publication_id=? ORDER BY i.route_code,i.driver_name"
            ),
            [s(&row, "id")],
        )?
        .iter()
        .map(itinerary_from)
        .collect();
    Ok(Some(RouteDayView {
        publication: publication(&row),
        itineraries,
    }))
}

/// One driver's itinerary for a day: its stops with their tasks and locations, its
/// breaks, its unknown stops and the tasks removed from it.
pub(super) fn route_itinerary(
    store: &Store,
    id: &str,
    day: &str,
    itinerary_id: &str,
) -> Result<Option<RouteItineraryDetail>> {
    dispatch_core::foundation::validate::date(day)?;
    let station = store.profile(id)?.station_code;
    let db = store.routes_db(id)?;
    let Some(row) = db.one(
        &format!("{PUBLICATION_SELECT} WHERE station=? AND day=? AND active=1"),
        [&station, day],
    )?
    else {
        return Ok(None);
    };
    let publication_id = s(&row, "id").to_owned();
    let Some(itinerary) = db
        .one(
            &format!("{ITINERARY_SELECT} WHERE i.publication_id=? AND i.itinerary_id=?"),
            [&publication_id, itinerary_id],
        )?
        .as_ref()
        .map(itinerary_from)
    else {
        return Ok(None);
    };
    // An old day whose task rows were let go has them in its itinerary's stored response.
    let pruned = db.count(
        "SELECT tasks_pruned FROM route_publications WHERE id=?",
        [&publication_id],
    )? == 1;
    let tasks = if pruned {
        stored_tasks(&db, &publication_id, itinerary_id)?
    } else {
        db.all(
            "SELECT t.*,a.address1,a.city,a.state,a.postal_code,a.latitude address_latitude,a.longitude address_longitude \
             FROM tasks t LEFT JOIN addresses a ON a.address_id=t.address_id \
             WHERE t.publication_id=? AND t.itinerary_id=? ORDER BY t.executed_at,t.task_id",
            [&publication_id, itinerary_id],
        )?
    };
    // Preserve the query's task ordering while indexing each active task once.
    let mut by_stop: HashMap<&str, Vec<&Value>> = HashMap::new();
    for task in &tasks {
        if task["active"] != 0
            && let Some(stop) = task["stop_id"].as_str()
        {
            by_stop.entry(stop).or_default().push(task);
        }
    }
    let stops = db
        .all(
            "SELECT s.*,a.address1,a.city,a.state,a.postal_code,a.latitude,a.longitude \
             FROM stops s LEFT JOIN addresses a ON a.address_id=s.address_id \
             WHERE s.publication_id=? AND s.itinerary_id=? ORDER BY s.sequence,s.stop_id",
            [&publication_id, itinerary_id],
        )?
        .iter()
        .map(|stop| RouteStop {
            stop_id: s(stop, "stop_id").into(),
            sequence: stop["sequence"].as_i64(),
            stop_type: stop["stop_type"].as_str().map(str::to_owned),
            planned_start_at: stop["planned_start_at"].as_i64(),
            planned_end_at: stop["planned_end_at"].as_i64(),
            address: address_from(stop, "latitude", "longitude"),
            tasks: by_stop
                .remove(s(stop, "stop_id"))
                .unwrap_or_default()
                .into_iter()
                .map(task_from)
                .collect(),
        })
        .collect();
    let breaks = db
        .all(
            "SELECT * FROM breaks WHERE publication_id=? AND itinerary_id=? ORDER BY planned,ordinal",
            [&publication_id, itinerary_id],
        )?
        .iter()
        .map(|b| RouteBreak {
            planned: b["planned"] == 1,
            break_id: b["break_id"].as_str().map(str::to_owned),
            kind: b["kind"].as_str().map(str::to_owned),
            state: b["state"].as_str().map(str::to_owned),
            started_at: b["started_at"].as_i64(),
            ended_at: b["ended_at"].as_i64(),
            planned_start_at: b["planned_start_at"].as_i64(),
            planned_end_at: b["planned_end_at"].as_i64(),
        })
        .collect();
    let unknown_stops = db
        .all(
            "SELECT * FROM unknown_stops WHERE publication_id=? AND itinerary_id=? ORDER BY ordinal",
            [&publication_id, itinerary_id],
        )?
        .iter()
        .map(|u| RouteUnknownStop {
            entered_at: u["entered_at"].as_i64(),
            exited_at: u["exited_at"].as_i64(),
            enter_latitude: u["enter_latitude"].as_f64(),
            enter_longitude: u["enter_longitude"].as_f64(),
            exit_latitude: u["exit_latitude"].as_f64(),
            exit_longitude: u["exit_longitude"].as_f64(),
        })
        .collect();
    Ok(Some(RouteItineraryDetail {
        publication: publication(&row),
        itinerary,
        stops,
        breaks,
        unknown_stops,
        inactive_tasks: tasks
            .iter()
            .filter(|t| t["active"] == 0)
            .map(task_from)
            .collect(),
    }))
}

/// Everything that happened to a package across the days stored, newest first.
pub(super) fn route_package(store: &Store, id: &str, tracking: &str) -> Result<RoutePackage> {
    let db = store.routes_db(id)?;
    let mut rows = db.all(
        "SELECT t.*,i.driver_name,i.route_code,a.address1,a.city,a.state,a.postal_code,\
         a.latitude address_latitude,a.longitude address_longitude \
         FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1 \
         LEFT JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id \
         LEFT JOIN addresses a ON a.address_id=t.address_id \
         WHERE t.tracking_id=? ORDER BY t.day DESC,t.executed_at,t.task_id LIMIT ?",
        params![tracking, MOST_EVENTS],
    )?;
    // On an old day whose delivered and picked-up task rows were let go, the rows it kept are
    // read above, and the rest of the package is in the responses of the itineraries that
    // day's lookup names for it: each task once, whichever way it was read.
    let old = db.all(
        "SELECT d.publication_id,d.itinerary_id,i.driver_name,i.route_code FROM package_days d \
         JOIN route_publications p ON p.id=d.publication_id AND p.active=1 AND p.tasks_pruned=1 \
         LEFT JOIN itineraries i ON i.publication_id=d.publication_id AND i.itinerary_id=d.itinerary_id \
         WHERE d.tracking_id=? ORDER BY p.day DESC LIMIT ?",
        params![tracking, MOST_EVENTS],
    )?;
    let task_of = |task: &Value| {
        (
            s(task, "publication_id").to_owned(),
            s(task, "itinerary_id").to_owned(),
            s(task, "task_id").to_owned(),
        )
    };
    let mut read: std::collections::HashSet<_> = rows.iter().map(task_of).collect();
    // Newest first, until the answer's five hundred events are found: each itinerary named
    // holds at least one, so no more are ever read than a whole answer needs.
    let mut found = 0;
    for held in &old {
        if found >= MOST_EVENTS {
            break;
        }
        for mut task in stored_tasks(&db, s(held, "publication_id"), s(held, "itinerary_id"))? {
            if task["tracking_id"] == tracking && read.insert(task_of(&task)) {
                task["driver_name"] = held["driver_name"].clone();
                task["route_code"] = held["route_code"].clone();
                rows.push(task);
                found += 1;
            }
        }
    }
    if !old.is_empty() {
        // As the rows are ordered: newest day first, then by when it happened.
        rows.sort_by(|a, b| {
            (s(b, "day").cmp(s(a, "day"))).then_with(|| {
                (a["executed_at"].as_i64(), s(a, "task_id"))
                    .cmp(&(b["executed_at"].as_i64(), s(b, "task_id")))
            })
        });
        rows.truncate(MOST_EVENTS as usize);
    }
    let events = rows
        .iter()
        .map(|t| RoutePackageEvent {
            day: s(t, "day").into(),
            transporter_id: s(t, "transporter_id").into(),
            driver_name: t["driver_name"].as_str().map(str::to_owned),
            route_code: t["route_code"].as_str().map(str::to_owned),
            itinerary_id: s(t, "itinerary_id").into(),
            task: task_from(t),
            address: address_from(t, "address_latitude", "address_longitude"),
        })
        .collect();
    Ok(RoutePackage {
        tracking_id: tracking.into(),
        events,
    })
}

/// The itinerary and day-summary columns a `RouteItinerary` is read from.
const ITINERARY_SELECT: &str = "SELECT i.itinerary_id,i.transporter_id,i.driver_name,\
    i.route_code,i.execution_status,i.progress_status,\
    d.departed_at,d.first_stop_at,d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.tasks_total,\
    d.delivered,d.picked_up,d.not_delivered,d.breaks_secs,d.overtime_secs \
    FROM itineraries i JOIN driver_days d ON d.publication_id=i.publication_id AND d.itinerary_id=i.itinerary_id";
/// The events a package's history answers with at most, newest first.
const MOST_EVENTS: i64 = 500;
/// An itinerary's tasks as its rows had them, read from its stored response: for a day whose
/// task rows were let go. Each as a row of `tasks` joined to its address, in the rows' order,
/// with the itinerary's day and driver; a task both on the route and removed from it is the
/// one on the route, as its rows kept it. None for an itinerary the day holds no response of.
fn stored_tasks(db: &Db, publication_id: &str, itinerary_id: &str) -> Result<Vec<Value>> {
    let Some(publication) = db.one(
        "SELECT * FROM route_publications WHERE id=?",
        [publication_id],
    )?
    else {
        return Ok(Vec::new());
    };
    // Its lists and this one response: no other driver's is read.
    let capture = stored_itinerary(db, &publication, itinerary_id)?;
    let Some(captured) = capture.itineraries.first() else {
        return Ok(Vec::new());
    };
    let shaped = Shaper::new(&capture).shape(captured)?;
    let mut seen = std::collections::HashSet::new();
    let mut tasks = Vec::with_capacity(shaped.tasks.len() + shaped.inactive.len());
    let rows = (shaped.tasks.iter().map(|task| (task, 1)))
        .chain(shaped.inactive.iter().map(|task| (task, 0)));
    for (task, active) in rows {
        if !seen.insert(task.id.as_str()) {
            continue;
        }
        let mut row = serde_json::to_value(task)?;
        row["task_id"] = json!(task.id);
        row["active"] = json!(active);
        row["day"] = publication["day"].clone();
        row["publication_id"] = json!(publication_id);
        row["itinerary_id"] = json!(itinerary_id);
        row["transporter_id"] = json!(shaped.transporter_id);
        if let Some(address) = &task.address_id
            && let Some(kept) = db.one(
                "SELECT address1,city,state,postal_code,latitude address_latitude,\
                 longitude address_longitude FROM addresses WHERE address_id=?",
                [address],
            )?
        {
            for (field, value) in kept.as_object().into_iter().flatten() {
                row[field] = value.clone();
            }
        }
        tasks.push(row);
    }
    // As `ORDER BY executed_at,task_id` has them: those without a time first.
    tasks.sort_by(|a, b| {
        (a["executed_at"].as_i64(), s(a, "task_id"))
            .cmp(&(b["executed_at"].as_i64(), s(b, "task_id")))
    });
    Ok(tasks)
}

fn itinerary_from(r: &Value) -> RouteItinerary {
    RouteItinerary {
        itinerary_id: s(r, "itinerary_id").into(),
        transporter_id: s(r, "transporter_id").into(),
        driver_name: s(r, "driver_name").into(),
        route_code: r["route_code"].as_str().map(str::to_owned),
        execution_status: r["execution_status"].as_str().map(str::to_owned),
        progress_status: r["progress_status"].as_str().map(str::to_owned),
        departed_at: r["departed_at"].as_i64(),
        first_stop_at: r["first_stop_at"].as_i64(),
        last_stop_at: r["last_stop_at"].as_i64(),
        session_end_at: r["session_end_at"].as_i64(),
        stops_total: r["stops_total"].as_i64().unwrap_or(0),
        stops_completed: r["stops_completed"].as_i64().unwrap_or(0),
        tasks_total: r["tasks_total"].as_i64().unwrap_or(0),
        delivered: r["delivered"].as_i64().unwrap_or(0),
        picked_up: r["picked_up"].as_i64().unwrap_or(0),
        not_delivered: r["not_delivered"].as_i64().unwrap_or(0),
        breaks_secs: r["breaks_secs"].as_i64(),
        overtime_secs: r["overtime_secs"].as_i64(),
    }
}
fn task_from(t: &Value) -> RouteTask {
    RouteTask {
        task_id: s(t, "task_id").into(),
        tracking_id: t["tracking_id"].as_str().map(str::to_owned),
        order_id: t["order_id"].as_str().map(str::to_owned),
        task_type: t["task_type"].as_str().map(str::to_owned),
        task_state: t["task_state"].as_str().map(str::to_owned),
        state_context: t["state_context"].as_str().map(str::to_owned),
        execution_status: t["execution_status"].as_str().map(str::to_owned),
        executed_at: t["executed_at"].as_i64(),
        window_start_at: t["window_start_at"].as_i64(),
        window_end_at: t["window_end_at"].as_i64(),
        latitude: t["latitude"].as_f64(),
        longitude: t["longitude"].as_f64(),
        address_id: t["address_id"].as_str().map(str::to_owned),
        address_type: t["address_type"].as_str().map(str::to_owned),
        active: t["active"] != 0,
    }
}
/// The address joined onto a row, when the row has one.
fn address_from(row: &Value, latitude: &str, longitude: &str) -> Option<RouteAddress> {
    let address_id = row["address_id"].as_str()?;
    Some(RouteAddress {
        address_id: address_id.into(),
        address1: row["address1"].as_str().map(str::to_owned),
        city: row["city"].as_str().map(str::to_owned),
        state: row["state"].as_str().map(str::to_owned),
        postal_code: row["postal_code"].as_str().map(str::to_owned),
        latitude: row[latitude].as_f64(),
        longitude: row[longitude].as_f64(),
    })
}

/// The columns a `RoutePublication` is read from.
pub(super) const PUBLICATION_SELECT: &str = "SELECT id,day,mode,station,collected_at,\
    route_count,itinerary_count,stop_count,task_count FROM route_publications";
pub(super) fn publication(row: &Value) -> RoutePublication {
    RoutePublication {
        id: s(row, "id").into(),
        day: s(row, "day").into(),
        mode: s(row, "mode").into(),
        station: s(row, "station").into(),
        collected_at: s(row, "collected_at").into(),
        route_count: row["route_count"].as_i64().unwrap_or(0),
        itinerary_count: row["itinerary_count"].as_i64().unwrap_or(0),
        stop_count: row["stop_count"].as_i64().unwrap_or(0),
        task_count: row["task_count"].as_i64().unwrap_or(0),
    }
}
