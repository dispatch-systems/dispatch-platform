//! Writing a captured day into the database: its route list, each itinerary with its stops,
//! tasks and packages as `rows` flattens them, and every response whole, compressed; then
//! making it the day's read. A day being staged has its rows added under a publication of its
//! own; live route data's one day is kept in place, read by read.
use super::*;
use rusqlite::types::Value as SqlValue;

/// How rows are written: added under a publication being staged; or kept in place in the one
/// publication live route data holds, where a row new to the read is inserted, one that changed
/// is updated, and one unchanged is left unwritten.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Writes {
    Staged,
    InPlace,
}

/// Makes a staged day its scope's read, in one transaction, if it was collected later than
/// the read the day has, in whatever order the two are published: the day's last read is
/// left inactive, what the new one sent of the drivers and addresses the days share is
/// written over them, and the day's work phones become `phones`, the new read's. A day
/// collected no later is left inactive itself, changing none of them. Either way
/// `sweep_routes` deletes what's inactive in small steps; only flags and the day's shared
/// rows and phones change here, so the platform lock is held for moments.
/// Answers whether it is the day's read now.
pub(super) fn publish_day(
    db: &Db,
    job: &str,
    staged: &Staged,
    phones: &[WorkPhone],
) -> Result<bool> {
    db.transaction(|| {
        let row = db
            .one(
                "SELECT collected_at,itinerary_count,\
                 (SELECT count(*) FROM itineraries WHERE publication_id=p.id) stored \
                 FROM route_publications p WHERE id=? AND job_id=? AND active=0",
                [&staged.publication, job],
            )?
            .ok_or_else(|| Error::new("routes_capture_invalid", 502))?;
        ensure(
            row["itinerary_count"] == row["stored"] && row["stored"] == json!(staged.itineraries),
            "routes_capture_invalid",
            502,
        )?;
        let scope = "day=? AND station=? AND service_area_id=? AND provider=? AND active=1";
        let later = db.one(
            &format!("SELECT 1 FROM route_publications WHERE {scope} AND collected_at>=?"),
            params![
                staged.day,
                staged.station,
                staged.service_area_id,
                staged.provider,
                s(&row, "collected_at")
            ],
        )?;
        if later.is_some() {
            return Ok(false);
        }
        db.exec(
            &format!("UPDATE route_publications SET active=0 WHERE {scope}"),
            params![
                staged.day,
                staged.station,
                staged.service_area_id,
                staged.provider
            ],
        )?;
        db.exec(
            "UPDATE route_publications SET active=1 WHERE id=?",
            [&staged.publication],
        )?;
        share(db, &staged.publication, &staged.day)?;
        db.exec("DELETE FROM work_phones WHERE day=?", [&staged.day])?;
        let mut insert = db.0.prepare_cached(
            "INSERT INTO work_phones(day,transporter_id,phone,since,until) VALUES (?,?,?,?,?)",
        )?;
        for hold in phones {
            insert.execute(params![
                staged.day,
                hold.transporter_id,
                hold.phone,
                hold.since,
                hold.until
            ])?;
        }
        Ok(true)
    })
}

const ROUTE_KEY: &[&str] = &["publication_id", "route_id"];
const ROUTE_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "route_id",
    "rms_route_id",
    "route_code",
    "company_id",
    "service_type",
    "route_status",
    "progress_status",
    "planned_departure_at",
    "created_at",
    "duration_secs",
    "total_stops",
    "total_tasks",
    "unassigned_stops",
    "unassigned_packages",
    "late_departing",
    "pre_dispatch",
    "same_day",
    "transporter_ids",
    "summary",
];
const RAW_KEY: &[&str] = &["publication_id", "name"];
const RAW_COLUMNS: &[&str] = &["publication_id", "name", "encoding", "raw_bytes", "body"];
const ITINERARY_KEY: &[&str] = &["publication_id", "itinerary_id"];
const STOP_KEY: &[&str] = &["publication_id", "itinerary_id", "stop_id"];
const STOP_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "itinerary_id",
    "stop_id",
    "sequence",
    "address_id",
    "stop_type",
    "route_code",
    "planned_start_at",
    "planned_end_at",
    "expected_start_at",
    "actual_start_at",
    "flags",
];
const TASK_KEY: &[&str] = &["publication_id", "itinerary_id", "task_id"];
const TASK_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "itinerary_id",
    "stop_id",
    "task_id",
    "transporter_id",
    "tracking_id",
    "order_id",
    "task_type",
    "task_state",
    "state_context",
    "execution_status",
    "address_id",
    "window_start_at",
    "window_end_at",
    "executed_at",
    "latitude",
    "longitude",
    "box_type",
    "weight",
    "weight_unit",
    "length",
    "width",
    "height",
    "volume_unit",
    "time_windowed",
    "high_value",
    "customer_return",
    "address_type",
    "events",
    "active",
];
const DAY_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "transporter_id",
    "itinerary_id",
    "route_code",
    "departed_at",
    "first_stop_at",
    "last_stop_at",
    "session_end_at",
    "stops_total",
    "stops_completed",
    "tasks_total",
    "delivered",
    "picked_up",
    "not_delivered",
    "breaks_secs",
    "overtime_secs",
];
const BREAK_KEY: &[&str] = &["publication_id", "itinerary_id", "planned", "ordinal"];
const BREAK_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "itinerary_id",
    "transporter_id",
    "ordinal",
    "planned",
    "break_id",
    "kind",
    "state",
    "sequence",
    "punch_id",
    "started_at",
    "ended_at",
    "planned_start_at",
    "planned_end_at",
    "min_duration_ms",
];
const UNKNOWN_KEY: &[&str] = &["publication_id", "itinerary_id", "ordinal"];
const UNKNOWN_COLUMNS: &[&str] = &[
    "publication_id",
    "day",
    "itinerary_id",
    "transporter_id",
    "ordinal",
    "entered_at",
    "exited_at",
    "enter_latitude",
    "enter_longitude",
    "exit_latitude",
    "exit_longitude",
];
/// What a read sends of an address and a driver the days share, beside the days each was seen.
const ADDRESS_COLUMNS: &[&str] = &[
    "address1",
    "address2",
    "address3",
    "city",
    "state",
    "postal_code",
    "latitude",
    "longitude",
];
const DRIVER_COLUMNS: &[&str] = &[
    "first_name",
    "last_name",
    "initials",
    "work_phone",
    "person_type",
    "company_id",
];
/// What the route list sends of a driver it assigns: their phone and company come with their
/// itinerary.
const ASSIGNED_COLUMNS: &[&str] = &["first_name", "last_name", "initials", "person_type"];

fn list(items: impl IntoIterator<Item = String>) -> String {
    items.into_iter().collect::<Vec<_>>().join(",")
}
/// The statement writing one row of `table`'s `columns`: as `insert` adds it, staged; in place,
/// inserted when its `key` is new, and otherwise updated only where another column differs.
fn statement(write: Writes, insert: &str, table: &str, key: &[&str], columns: &[&str]) -> String {
    let all = columns.join(",");
    let values = vec!["?"; columns.len()].join(",");
    if write == Writes::Staged {
        return format!("{insert} INTO {table}({all}) VALUES ({values})");
    }
    let rest: Vec<&str> = columns
        .iter()
        .copied()
        .filter(|c| !key.contains(c))
        .collect();
    format!(
        "INSERT INTO {table}({all}) VALUES ({values}) ON CONFLICT({}) DO UPDATE SET {} \
         WHERE ({}) IS NOT ({})",
        key.join(","),
        list(rest.iter().map(|c| format!("{c}=excluded.{c}"))),
        list(rest.iter().map(|c| format!("{table}.{c}"))),
        list(rest.iter().map(|c| format!("excluded.{c}"))),
    )
}
/// The statement writing a row the days share over the one there, in place, as `share` does from
/// staging: inserted when its `key` is new, and otherwise its `columns` written and the days it
/// was seen widened, only where that changes something.
fn shared(table: &str, key: &str, columns: &[&str]) -> String {
    let seen = ["first_seen_day", "last_seen_day"];
    let widened = [
        format!("min({table}.first_seen_day,excluded.first_seen_day)"),
        format!("max({table}.last_seen_day,excluded.last_seen_day)"),
    ];
    let all: Vec<&str> = std::iter::once(key)
        .chain(columns.iter().copied())
        .chain(seen)
        .collect();
    let values = vec!["?"; all.len()].join(",");
    let set = columns
        .iter()
        .map(|c| format!("{c}=excluded.{c}"))
        .chain(seen.iter().zip(&widened).map(|(c, w)| format!("{c}={w}")));
    let now = columns.iter().chain(&seen).map(|c| format!("{table}.{c}"));
    let then = columns
        .iter()
        .map(|c| format!("excluded.{c}"))
        .chain(widened.iter().cloned());
    format!(
        "INSERT INTO {table}({}) VALUES ({values}) ON CONFLICT({key}) DO UPDATE SET {} \
         WHERE ({}) IS NOT ({})",
        all.join(","),
        list(set),
        list(now),
        list(then),
    )
}
/// A driver's kinds of person, as their row keeps them.
fn person_type(person: &Value) -> String {
    person["personType"]
        .as_array()
        .map(|v| Value::Array(v.clone()).to_string())
        .unwrap_or_else(|| "[]".into())
}

/// What a read sends of a row the days share, as its statement takes it: staged under
/// `publication`, or in place with `day`, the day it was seen.
fn shared_row(
    write: Writes,
    publication: &str,
    day: &str,
    sent: Vec<SqlValue>,
) -> rusqlite::ParamsFromIter<Vec<SqlValue>> {
    let row = match write {
        Writes::Staged => std::iter::once(publication.to_owned().into())
            .chain(sent)
            .collect(),
        Writes::InPlace => (sent.into_iter())
            .chain([day.to_owned().into(), day.to_owned().into()])
            .collect(),
    };
    rusqlite::params_from_iter(row)
}
/// The key a planned route's row is stored under.
fn route_id(route: &Value) -> String {
    text(&route["routeId"]).unwrap_or_else(|| text(&route["rmsRouteId"]).unwrap_or_default())
}
/// The day's planned routes, inside the caller's transaction. Answers the rows written.
pub(super) fn write_routes(
    db: &Db,
    write: Writes,
    publication: &str,
    day: &str,
    routes: &[Value],
) -> Result<usize> {
    let mut write_route = db.0.prepare_cached(&statement(
        write,
        "INSERT OR REPLACE",
        "routes",
        ROUTE_KEY,
        ROUTE_COLUMNS,
    ))?;
    let mut written = 0;
    for route in routes {
        let transporters: Vec<Value> = route["transporters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["transporterId"].as_str().map(|v| json!(v)))
            .collect();
        written += write_route.execute(params![
            publication,
            day,
            route_id(route),
            text(&route["rmsRouteId"]),
            text(&route["routeCode"]),
            text(&route["companyId"]),
            text(&route["serviceTypeName"]),
            text(&route["routeStatus"]),
            text(&route["progressStatus"]),
            stamp(&route["plannedDepartureTime"]),
            stamp(&route["routeCreationTime"]),
            integer(&route["routeDuration"]),
            integer(&route["totalStops"]),
            integer(&route["totalTasks"]),
            integer(&route["unassignedStops"]),
            integer(&route["unassignedPackages"]),
            flag(&route["lateDeparting"]),
            flag(&route["preDispatch"]),
            flag(&route["sameDayRoute"]),
            Value::Array(transporters).to_string(),
            route.to_string()
        ])?;
    }
    Ok(written)
}
/// Deletes the planned routes live route data's day holds that `routes` no longer lists. Answers
/// the rows deleted.
pub(super) fn prune_routes(db: &Db, publication: &str, routes: &[Value]) -> Result<usize> {
    let kept: Vec<String> = routes.iter().map(route_id).collect();
    db.exec(
        "DELETE FROM routes WHERE publication_id=?1 AND route_id NOT IN (SELECT value FROM json_each(?2))",
        params![publication, serde_json::to_string(&kept)?],
    )
}
/// The drivers the day's route list assigns who have none of `itineraries`, the read's, written
/// over the rows of the drivers live route data names where they changed, inside the caller's
/// transaction: a driver assigned to a route is known before their itinerary is read, which
/// names them from then on. Their work phone is left as it was: before a driver signs in, the
/// list shows the last phone they had. Answers the rows written.
pub(super) fn write_assigned(
    db: &Db,
    day: &str,
    route_summaries: &Value,
    itineraries: &[PreparedItinerary],
) -> Result<usize> {
    let mut upsert =
        db.0.prepare_cached(&shared("drivers", "transporter_id", ASSIGNED_COLUMNS))?;
    let mut written = 0;
    for person in route_summaries["transporters"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(id) = text(&person["transporterId"]) else {
            continue;
        };
        if itineraries.iter().any(|i| i.transporter_id == id) {
            continue;
        }
        written += upsert.execute(params![
            id,
            text(&person["firstName"]),
            text(&person["lastName"]),
            text(&person["initials"]),
            person_type(person),
            day,
            day
        ])?;
    }
    Ok(written)
}
/// A stored response, compressed, inside the caller's transaction. Answers the rows written.
pub(super) fn write_raw(
    db: &Db,
    write: Writes,
    publication: &str,
    name: &str,
    raw_bytes: usize,
    body: &[u8],
) -> Result<usize> {
    let sql = statement(write, "INSERT", "route_raw", RAW_KEY, RAW_COLUMNS);
    Ok((db.0.prepare_cached(&sql)?).execute(params![
        publication,
        name,
        "gzip",
        raw_bytes as i64,
        body
    ])?)
}
/// One shaped itinerary's rows under `publication`, inside the caller's transaction, with what
/// it sends of the drivers and addresses the days share: staged beside them until `share`, or in
/// place written over the shared rows where they changed. Answers the rows written. Its
/// compressed response is stored beside them by staging, and kept by a reprocess.
pub(super) fn write_itinerary(
    db: &Db,
    write: Writes,
    publication: &str,
    day: &str,
    itinerary: &PreparedItinerary,
) -> Result<usize> {
    let connection = &db.0;
    let prepare = |insert: &str, table: &str, key: &[&str], columns: &[&str]| {
        connection.prepare_cached(&statement(write, insert, table, key, columns))
    };
    let mut write_stop = prepare("INSERT OR REPLACE", "stops", STOP_KEY, STOP_COLUMNS)?;
    // Keyed by itinerary: the same package removed from one driver and active on
    // another is a row under each. Within one itinerary the active row wins.
    let mut write_task = prepare("INSERT OR IGNORE", "tasks", TASK_KEY, TASK_COLUMNS)?;
    let mut write_day = prepare("INSERT", "driver_days", ITINERARY_KEY, DAY_COLUMNS)?;
    let mut write_break = prepare("INSERT", "breaks", BREAK_KEY, BREAK_COLUMNS)?;
    let mut write_unknown = prepare("INSERT", "unknown_stops", UNKNOWN_KEY, UNKNOWN_COLUMNS)?;
    let mut columns = vec!["publication_id", "day", "itinerary_id"];
    columns.extend(itinerary.columns.iter().map(|(c, _)| c.as_str()));
    let mut bound: Vec<Value> = vec![json!(publication), json!(day), json!(itinerary.id)];
    bound.extend(itinerary.columns.iter().map(|(_, v)| v.clone()));
    let mut written = prepare("INSERT", "itineraries", ITINERARY_KEY, &columns)?
        .execute(rusqlite::params_from_iter(bound.iter().map(sql_value)))?;
    for stop in &itinerary.stops {
        written += write_stop.execute(params![
            publication,
            day,
            itinerary.id,
            stop.id,
            stop.sequence,
            stop.address_id,
            stop.stop_type,
            stop.route_code,
            stop.planned_start_at,
            stop.planned_end_at,
            stop.expected_start_at,
            stop.actual_start_at,
            stop.flags.to_string()
        ])?;
    }
    for (task, active) in tasks(itinerary) {
        written += write_task.execute(params![
            publication,
            day,
            itinerary.id,
            task.stop_id,
            task.id,
            itinerary.transporter_id,
            task.tracking_id,
            task.order_id,
            task.task_type,
            task.task_state,
            task.state_context,
            task.execution_status,
            task.address_id,
            task.window_start_at,
            task.window_end_at,
            task.executed_at,
            task.latitude,
            task.longitude,
            task.box_type,
            task.weight,
            task.weight_unit,
            task.length,
            task.width,
            task.height,
            task.volume_unit,
            task.time_windowed,
            task.high_value,
            task.customer_return,
            task.address_type,
            task.events.to_string(),
            active
        ])?;
    }
    let d = &itinerary.day;
    written += write_day.execute(params![
        publication,
        day,
        itinerary.transporter_id,
        itinerary.id,
        d.route_code,
        d.departed_at,
        d.first_stop_at,
        d.last_stop_at,
        d.session_end_at,
        d.stops_total,
        d.stops_completed,
        d.tasks_total,
        d.delivered,
        d.picked_up,
        d.not_delivered,
        d.breaks_secs,
        d.overtime_secs
    ])?;
    let mut ordinals = [0i64, 0i64];
    for b in &itinerary.breaks {
        let ordinal = &mut ordinals[usize::from(b.planned)];
        written += write_break.execute(params![
            publication,
            day,
            itinerary.id,
            itinerary.transporter_id,
            *ordinal,
            i64::from(b.planned),
            b.break_id,
            b.kind,
            b.state,
            b.sequence,
            b.punch_id,
            b.started_at,
            b.ended_at,
            b.planned_start_at,
            b.planned_end_at,
            b.min_duration_ms
        ])?;
        *ordinal += 1;
    }
    for (ordinal, u) in itinerary.unknown_stops.iter().enumerate() {
        written += write_unknown.execute(params![
            publication,
            day,
            itinerary.id,
            itinerary.transporter_id,
            ordinal as i64,
            u.entered_at,
            u.exited_at,
            u.enter_latitude,
            u.enter_longitude,
            u.exit_latitude,
            u.exit_longitude
        ])?;
    }
    let (mut write_address, mut write_driver) = match write {
        Writes::Staged => (
            connection.prepare_cached(
                "INSERT OR REPLACE INTO staged_addresses(publication_id,address_id,address1,address2,\
                 address3,city,state,postal_code,latitude,longitude) VALUES (?,?,?,?,?,?,?,?,?,?)",
            )?,
            connection.prepare_cached(
                "INSERT OR REPLACE INTO staged_drivers(publication_id,transporter_id,first_name,\
                 last_name,initials,work_phone,person_type,company_id) VALUES (?,?,?,?,?,?,?,?)",
            )?,
        ),
        Writes::InPlace => (
            connection.prepare_cached(&shared("addresses", "address_id", ADDRESS_COLUMNS))?,
            connection.prepare_cached(&shared("drivers", "transporter_id", DRIVER_COLUMNS))?,
        ),
    };
    for address in &itinerary.addresses {
        let Some(address_id) = text(&address["addressId"]) else {
            continue;
        };
        let sent = vec![
            address_id.into(),
            text(&address["address1"]).into(),
            text(&address["address2"]).into(),
            text(&address["address3"]).into(),
            text(&address["city"]).into(),
            text(&address["state"]).into(),
            text(&address["postalCode"]).into(),
            number(&address["geocode"]["latitude"]).into(),
            number(&address["geocode"]["longitude"]).into(),
        ];
        written += write_address.execute(shared_row(write, publication, day, sent))?;
    }
    let person = &itinerary.person;
    if !person.is_null() {
        let company = itinerary
            .columns
            .iter()
            .find(|(c, _)| c == "company_id")
            .and_then(|(_, v)| v.as_str().map(str::to_owned));
        let sent = vec![
            itinerary.transporter_id.clone().into(),
            text(&person["firstName"]).into(),
            text(&person["lastName"]).into(),
            text(&person["initials"]).into(),
            text(&person["workPhoneNumber"]).into(),
            person_type(person).into(),
            company.into(),
        ];
        written += write_driver.execute(shared_row(write, publication, day, sent))?;
    }
    Ok(written)
}
/// An itinerary's tasks with whether each is active: its active ones, then those removed from it
/// that it doesn't also hold active, since within one itinerary the active row wins.
fn tasks(itinerary: &PreparedItinerary) -> impl Iterator<Item = (&TaskRow, i64)> {
    let active: std::collections::HashSet<&str> =
        itinerary.tasks.iter().map(|t| t.id.as_str()).collect();
    let removed = itinerary.inactive.iter();
    let removed = removed.filter(move |t| !active.contains(t.id.as_str()));
    itinerary
        .tasks
        .iter()
        .map(|t| (t, 1))
        .chain(removed.map(|t| (t, 0)))
}
/// Deletes what live route data's day holds of an itinerary that its read no longer does: stops
/// and tasks gone from it, and breaks and unknown stops past their count. Answers the rows
/// deleted.
pub(super) fn prune_itinerary(
    db: &Db,
    publication: &str,
    itinerary: &PreparedItinerary,
) -> Result<usize> {
    let stops: Vec<&str> = itinerary.stops.iter().map(|s| s.id.as_str()).collect();
    let tasks: Vec<&str> = tasks(itinerary).map(|(t, _)| t.id.as_str()).collect();
    let planned = itinerary.breaks.iter().filter(|b| b.planned).count();
    let taken = itinerary.breaks.len() - planned;
    let id = &itinerary.id;
    Ok(db.exec(
        "DELETE FROM stops WHERE publication_id=?1 AND itinerary_id=?2 \
         AND stop_id NOT IN (SELECT value FROM json_each(?3))",
        params![publication, id, serde_json::to_string(&stops)?],
    )? + db.exec(
        "DELETE FROM tasks WHERE publication_id=?1 AND itinerary_id=?2 \
         AND task_id NOT IN (SELECT value FROM json_each(?3))",
        params![publication, id, serde_json::to_string(&tasks)?],
    )? + db.exec(
        "DELETE FROM breaks WHERE publication_id=?1 AND itinerary_id=?2 \
         AND ordinal>=CASE planned WHEN 1 THEN ?4 ELSE ?3 END",
        params![publication, id, taken as i64, planned as i64],
    )? + db.exec(
        "DELETE FROM unknown_stops WHERE publication_id=?1 AND itinerary_id=?2 AND ordinal>=?3",
        params![publication, id, itinerary.unknown_stops.len() as i64],
    )?)
}
/// Deletes the itineraries live route data's day holds that its read no longer lists, with every
/// row and response of theirs. Answers the rows deleted.
pub(super) fn prune_itineraries(db: &Db, publication: &str, listed: &[String]) -> Result<usize> {
    let listed = serde_json::to_string(listed)?;
    let mut deleted = 0;
    for table in [
        "itineraries",
        "stops",
        "tasks",
        "driver_days",
        "breaks",
        "unknown_stops",
    ] {
        deleted += db.exec(
            &format!(
                "DELETE FROM {table} WHERE publication_id=?1 \
                 AND itinerary_id NOT IN (SELECT value FROM json_each(?2))"
            ),
            params![publication, listed],
        )?;
    }
    deleted += db.exec(
        "DELETE FROM route_raw WHERE publication_id=?1 AND name LIKE 'itinerary:%' \
         AND substr(name,11) NOT IN (SELECT value FROM json_each(?2))",
        params![publication, listed],
    )?;
    Ok(deleted)
}
/// Writes what `publication`'s read sent of the drivers and addresses the days share over
/// them, as its day's read, `day`, now is, widening the days each was seen; then drops it from
/// staging. Inside the caller's transaction.
pub(super) fn share(db: &Db, publication: &str, day: &str) -> Result<()> {
    db.exec(
        "INSERT INTO addresses(address_id,address1,address2,address3,city,state,postal_code,\
         latitude,longitude,first_seen_day,last_seen_day) \
         SELECT address_id,address1,address2,address3,city,state,postal_code,latitude,longitude,?2,?2 \
         FROM staged_addresses WHERE publication_id=?1 \
         ON CONFLICT(address_id) DO UPDATE SET address1=excluded.address1,address2=excluded.address2,\
         address3=excluded.address3,city=excluded.city,state=excluded.state,postal_code=excluded.postal_code,\
         latitude=excluded.latitude,longitude=excluded.longitude,\
         first_seen_day=min(first_seen_day,excluded.first_seen_day),\
         last_seen_day=max(last_seen_day,excluded.last_seen_day)",
        params![publication, day],
    )?;
    db.exec(
        "INSERT INTO drivers(transporter_id,first_name,last_name,initials,work_phone,person_type,company_id,\
         first_seen_day,last_seen_day) \
         SELECT transporter_id,first_name,last_name,initials,work_phone,person_type,company_id,?2,?2 \
         FROM staged_drivers WHERE publication_id=?1 \
         ON CONFLICT(transporter_id) DO UPDATE SET \
         first_name=excluded.first_name,last_name=excluded.last_name,initials=excluded.initials,\
         work_phone=excluded.work_phone,person_type=excluded.person_type,company_id=excluded.company_id,\
         first_seen_day=min(first_seen_day,excluded.first_seen_day),\
         last_seen_day=max(last_seen_day,excluded.last_seen_day)",
        params![publication, day],
    )?;
    for table in ["staged_addresses", "staged_drivers"] {
        db.exec(
            &format!("DELETE FROM {table} WHERE publication_id=?"),
            [publication],
        )?;
    }
    Ok(())
}
