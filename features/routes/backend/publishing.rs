//! Writing a captured day into the database: its route list, each itinerary with its stops,
//! tasks and packages as `rows` flattens them, and every response whole, compressed.
use super::*;

/// The day's planned routes, inside the caller's transaction.
pub(super) fn insert_routes(db: &Db, publication: &str, day: &str, routes: &[Value]) -> Result<()> {
    let connection = &db.0;
    let mut insert_route = connection.prepare_cached(
        "INSERT OR REPLACE INTO routes(publication_id,day,route_id,rms_route_id,route_code,company_id,service_type,\
         route_status,progress_status,planned_departure_at,created_at,duration_secs,total_stops,total_tasks,\
         unassigned_stops,unassigned_packages,late_departing,pre_dispatch,same_day,transporter_ids,summary) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    for route in routes {
        let transporters: Vec<Value> = route["transporters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["transporterId"].as_str().map(|v| json!(v)))
            .collect();
        insert_route.execute(params![
            publication,
            day,
            text(&route["routeId"])
                .unwrap_or_else(|| text(&route["rmsRouteId"]).unwrap_or_default()),
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
    Ok(())
}
/// A stored response, compressed, inside the caller's transaction.
pub(super) fn insert_raw(
    db: &Db,
    publication: &str,
    name: &str,
    raw_bytes: usize,
    body: &[u8],
) -> Result<()> {
    db.0.prepare_cached(
        "INSERT INTO route_raw(publication_id,name,encoding,raw_bytes,body) VALUES (?,?,'gzip',?,?)",
    )?
    .execute(params![publication, name, raw_bytes as i64, body])?;
    Ok(())
}
/// One shaped itinerary's rows under `publication`, inside the caller's transaction.
/// Its compressed response is stored beside them by staging, and kept by a reprocess.
pub(super) fn insert_itinerary(
    db: &Db,
    publication: &str,
    day: &str,
    itinerary: &PreparedItinerary,
) -> Result<()> {
    let connection = &db.0;
    let mut insert_stop = connection.prepare_cached(
        "INSERT OR REPLACE INTO stops(publication_id,day,itinerary_id,stop_id,sequence,address_id,stop_type,route_code,\
         planned_start_at,planned_end_at,expected_start_at,actual_start_at,flags) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    // Keyed by itinerary: the same package removed from one driver and active on
    // another is a row under each. Within one itinerary the active row wins.
    let mut insert_task = connection.prepare_cached(
        "INSERT OR IGNORE INTO tasks(publication_id,day,itinerary_id,stop_id,task_id,transporter_id,tracking_id,order_id,\
         task_type,task_state,state_context,execution_status,address_id,window_start_at,window_end_at,executed_at,\
         latitude,longitude,box_type,weight,weight_unit,length,width,height,volume_unit,time_windowed,high_value,\
         customer_return,address_type,events,active) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut insert_day = connection.prepare_cached(
        "INSERT INTO driver_days(publication_id,day,transporter_id,itinerary_id,route_code,departed_at,first_stop_at,\
         last_stop_at,session_end_at,stops_total,stops_completed,tasks_total,delivered,picked_up,not_delivered,\
         breaks_secs,overtime_secs) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut insert_break = connection.prepare_cached(
        "INSERT INTO breaks(publication_id,day,itinerary_id,transporter_id,ordinal,planned,break_id,kind,state,sequence,\
         punch_id,started_at,ended_at,planned_start_at,planned_end_at,min_duration_ms) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut insert_unknown = connection.prepare_cached(
        "INSERT INTO unknown_stops(publication_id,day,itinerary_id,transporter_id,ordinal,entered_at,exited_at,\
         enter_latitude,enter_longitude,exit_latitude,exit_longitude) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut upsert_address = connection.prepare_cached(
        "INSERT INTO addresses(address_id,address1,address2,address3,city,state,postal_code,\
         latitude,longitude,first_seen_day,last_seen_day) VALUES (?,?,?,?,?,?,?,?,?,?,?) \
         ON CONFLICT(address_id) DO UPDATE SET address1=excluded.address1,address2=excluded.address2,\
         address3=excluded.address3,city=excluded.city,state=excluded.state,postal_code=excluded.postal_code,\
         latitude=excluded.latitude,longitude=excluded.longitude,\
         first_seen_day=min(first_seen_day,excluded.first_seen_day),\
         last_seen_day=max(last_seen_day,excluded.last_seen_day)",
    )?;
    let mut upsert_driver = connection.prepare_cached(
        "INSERT INTO drivers(transporter_id,first_name,last_name,initials,work_phone,person_type,company_id,\
         first_seen_day,last_seen_day) VALUES (?,?,?,?,?,?,?,?,?) ON CONFLICT(transporter_id) DO UPDATE SET \
         first_name=excluded.first_name,last_name=excluded.last_name,initials=excluded.initials,\
         work_phone=excluded.work_phone,person_type=excluded.person_type,company_id=excluded.company_id,\
         first_seen_day=min(first_seen_day,excluded.first_seen_day),\
         last_seen_day=max(last_seen_day,excluded.last_seen_day)",
    )?;
    let columns: Vec<&str> = itinerary.columns.iter().map(|(c, _)| c.as_str()).collect();
    let placeholders = vec!["?"; columns.len() + 3].join(",");
    let mut bound: Vec<Value> = vec![json!(publication), json!(day), json!(itinerary.id)];
    bound.extend(itinerary.columns.iter().map(|(_, v)| v.clone()));
    connection
        .prepare_cached(&format!(
            "INSERT INTO itineraries(publication_id,day,itinerary_id,{}) VALUES ({placeholders})",
            columns.join(",")
        ))?
        .execute(rusqlite::params_from_iter(bound.iter().map(sql_value)))?;
    for stop in &itinerary.stops {
        insert_stop.execute(params![
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
    for (task, active) in itinerary
        .tasks
        .iter()
        .map(|t| (t, 1))
        .chain(itinerary.inactive.iter().map(|t| (t, 0)))
    {
        insert_task.execute(params![
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
    insert_day.execute(params![
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
        insert_break.execute(params![
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
        insert_unknown.execute(params![
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
    for address in &itinerary.addresses {
        let Some(address_id) = text(&address["addressId"]) else {
            continue;
        };
        upsert_address.execute(params![
            address_id,
            text(&address["address1"]),
            text(&address["address2"]),
            text(&address["address3"]),
            text(&address["city"]),
            text(&address["state"]),
            text(&address["postalCode"]),
            number(&address["geocode"]["latitude"]),
            number(&address["geocode"]["longitude"]),
            day,
            day
        ])?;
    }
    let person = &itinerary.person;
    if !person.is_null() {
        let company = itinerary
            .columns
            .iter()
            .find(|(c, _)| c == "company_id")
            .and_then(|(_, v)| v.as_str().map(str::to_owned));
        upsert_driver.execute(params![
            itinerary.transporter_id,
            text(&person["firstName"]),
            text(&person["lastName"]),
            text(&person["initials"]),
            text(&person["workPhoneNumber"]),
            person["personType"]
                .as_array()
                .map(|v| Value::Array(v.clone()).to_string())
                .unwrap_or_else(|| "[]".into()),
            company,
            day,
            day
        ])?;
    }

    Ok(())
}
