//! What Routes holds of the synthetic DSP: each day's routes, through the collection's own
//! staging and publishing, so they read exactly as collected routes do.
use crate::{
    Error, Result,
    agents::synthetic::{Made, Step, Synthetic, World, context_at, packages_at, plan, tracking},
    collectors::cortex::{
        self,
        routes::{Capture, ItineraryCapture, Mode, Request},
    },
    db::{Store, s},
    routedata::RoutesStore,
};
use chrono::{Datelike, NaiveDate};
use serde_json::{Value, json};

pub const SYNTHETIC: Synthetic = Synthetic {
    people: None,
    steps: &[Step {
        order: 20,
        run: routes,
    }],
};

/// Routes, a day at a time, through the collection's own staging and publishing. Each
/// day's scope is left for the collections read beside it.
fn routes(db: &Store, world: &mut World) -> Result<Made> {
    world.connect(db, cortex::PROVIDER)?;
    let mut routes = 0;
    for (d, date) in world.dates.clone().into_iter().enumerate() {
        let day = date.to_string();
        let jobs = db.enqueue_routes(
            &world.dsp,
            None,
            &format!("synthetic:{day}"),
            Some(&day),
            Mode::Final,
            1,
        )?;
        let job = s(&jobs[0], "id").to_owned();
        let request: Value = serde_json::from_str(&db.job_row(&job, Some(&world.dsp))?.request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        let mut capture = cortex::routes::fixture(&request)?;
        let (summaries, route_summaries, itineraries) = routes_day(world, &capture, date, d as i64);
        routes += itineraries.len();
        capture.summaries["itinerarySummaries"] = summaries.0;
        capture.summaries["transporters"] = summaries.1;
        capture.route_summaries["rmsRouteSummaries"] = route_summaries;
        capture.route_summaries["transporters"] = capture.summaries["transporters"].clone();
        capture.itineraries = itineraries;
        world
            .scopes
            .insert(day, serde_json::to_value(&capture.scope)?);
        let staged = db.stage_routes(&world.dsp, &job, capture)?;
        db.publish_routes(&world.dsp, &job, &staged)?;
        world.collected(db, &job)?;
    }
    Ok(Some(("routes", json!(routes))))
}

/// A driver's stop is the same house every day, so feedback can repeat at an address.
fn address_id(driver: u64, stop: i64) -> String {
    format!("synthetic-address-{driver}-{stop}")
}

type Summaries = (Value, Value);
/// One day's lists and itineraries, in the shape Cortex answers with.
fn routes_day(
    world: &World,
    capture: &Capture,
    date: NaiveDate,
    d: i64,
) -> (Summaries, Value, Vec<ItineraryCapture>) {
    let provider = capture.scope.provider.clone();
    let area = capture.scope.service_area_id.clone();
    let companies = json!([{"companyId": provider, "companyName": "Synthetic Logistics",
        "companyShortCode": "SYN", "id": provider}]);
    let (mut summaries, mut people, mut routes, mut itineraries) = (vec![], vec![], vec![], vec![]);
    for (i, _, name) in &world.drivers {
        let Some(day) = plan(*i, d) else {
            continue;
        };
        let (first, last) = name.split_once(' ').unwrap_or((name, ""));
        let id = format!("synthetic-{date}-{i}");
        let route = format!("CX{}", 100 + i);
        let who = world.transporter(*i);
        let depart = world.at(date, day.departed);
        let stop_gap = day.length * 60_000 / day.stops.max(1);
        let mut packages = (0, 0);
        let mut stops = vec![
            json!({"stopId": format!("{id}-0"), "sequenceNumber": 0, "addressId": "station",
            "itineraryStopType": "PICK_UP", "routeCode": route, "plannedStartTime": depart - 1_800_000,
            "plannedEndTime": depart - 600_000, "expectedStartTime": depart - 1_800_000,
            "actualStartTime": null, "dispatchStopFlag": true, "tasks": []}),
        ];
        let mut addresses = vec![];
        for n in 1..=day.stops {
            let when = depart + 35 * 60_000 + n * stop_gap;
            let missed = n > day.stops - day.missed;
            let count = packages_at(*i, d, n);
            // As Cortex records them: a package brought back is BACK_TO_ORIGIN with Amazon's
            // reason, a delivered one says where it was left.
            let state = if missed {
                "BACK_TO_ORIGIN"
            } else {
                "DELIVERED"
            };
            let context = context_at(*i, d, n, missed);
            let tasks: Vec<Value> = (0..count)
                .map(|k| {
                    let tracking = tracking(date, *i, n, k);
                    json!({"taskId": format!("{id}-{n}-{k}"), "transporterId": null,
                        "referenceId": format!("{id}-{n}-{k}"), "taskType": "DROP_OFF", "taskState": state,
                        "taskStateContext": context,
                        "executionStatus": "COMPLETE", "addressId": address_id(*i, n),
                        "promiseType": "STANDARD", "windowStartTime": depart, "windowEndTime": depart + 43_200_000,
                        "actualExecutionTime": when, "executionGeocode": {"latitude": 41.8, "longitude": -87.6, "scope": 0},
                        "recentTaskEvents": [{"taskState": state, "taskStateContext": "NONE", "executionTime": when}],
                        "length": "10.0", "width": "5.0", "height": "4.0", "weight": "2.5", "volumeUnit": "IN",
                        "weightUnit": "LB", "boxType": "BOX", "timeWindowed": false, "highValueTaskFlag": false,
                        "customerReturn": false, "addressTypeInfo": "RESIDENTIAL",
                        "domainMap": {"scannableId": tracking, "orderId": format!("order-{id}-{n}-{k}"),
                            "trId": format!("{id}-{n}-{k}"), "SOURCE": "AMAZON"}})
                })
                .collect();
            if missed {
                packages.1 += count;
            } else {
                packages.0 += count;
            }
            addresses.push(json!({"addressId": address_id(*i, n), "address1": format!("{} Synthetic Ave", 1000 * i + n as u64),
                "address2": null, "address3": null, "city": "Example", "state": "IL", "postalCode": "60000",
                "geocode": {"latitude": 41.8, "longitude": -87.6, "scope": 0}}));
            stops.push(json!({"stopId": format!("{id}-{n}"), "sequenceNumber": n, "addressId": address_id(*i, n),
                "itineraryStopType": "DROP_OFF", "routeCode": route, "plannedStartTime": when,
                "plannedEndTime": when + 300_000, "expectedStartTime": when, "actualStartTime": when,
                "highValueStopFlag": false, "tasks": tasks}));
        }
        let total = packages.0 + packages.1;
        let completed = day.stops - day.missed;
        let session_end = world.at(date, day.last_stop() + 40);
        let breaks = if day.meal.is_some() { 1800 } else { 0 };
        let person = json!({"transporterId": who, "firstName": first, "lastName": last, "initials": "SD",
            "workPhoneNumber": "15550000000", "personType": ["DELIVERY_ASSOCIATE"], "lastLocation": null});
        summaries.push(json!({"itineraryId": id, "transporterId": who, "companyId": provider, "routeCode": route,
            "routeCodes": [route], "vinNumber": format!("SYNTHVIN{i:09}"), "executionStatus": "COMPLETE",
            "progressStatus": "ON_TIME", "serviceTypeName": "Standard Parcel", "plannedDepartureTime": depart,
            "itineraryStartTime": depart, "waveStartTime": depart - 1_800_000, "sessionEndTime": session_end,
            "projectedCompletionTime": session_end, "lastStopExecutionTime": world.at(date, day.last_stop()),
            "blockDurationInMinutes": 600, "totalLocations": day.stops, "completedLocations": completed,
            "totalPackages": total, "packageSummary": {"DELIVERED": packages.0, "REMAINING": 0,
            "UNDELIVERABLE": packages.1}, "stopsImpacted": day.missed, "packagesImpacted": packages.1,
            "totalBreaksDurationSecs": breaks, "overtimeDurationSecs": 0,
            "stopCompletionRate": completed as f64 / day.stops as f64,
            "stopProgress": {"completed": completed, "total": day.stops}, "breaks": [], "plannedBreaks": []}));
        routes.push(json!({"routeId": format!("{}", 5000 + i), "rmsRouteId": format!("rms-{id}"), "routeCode": route,
            "serviceAreaId": area, "companyId": provider, "serviceTypeName": "Standard Parcel",
            "routeStatus": "COMPLETE", "progressStatus": "ON_TIME", "plannedDepartureTime": depart,
            "routeCreationTime": depart - 21_600_000, "routeDuration": 36000, "totalStops": day.stops,
            "totalTasks": total, "unassignedStops": 0, "unassignedPackages": 0, "lateDeparting": false,
            "preDispatch": false, "sameDayRoute": false,
            "transporters": [{"transporterId": who, "itineraryId": id}]}));
        itineraries.push(ItineraryCapture {
            id: id.clone(),
            transporter_id: who.clone(),
            detail: json!({"itineraryDetails": {"itineraryId": id, "transporterId": who, "transporterName": name,
                "serviceAreaId": area, "companyId": provider,
                "localDate": [date.year(), date.month(), date.day()], "routeCode": route,
                "executionStatus": "COMPLETE", "progressStatus": "ON_TIME", "itineraryStartTime": depart,
                "plannedDepartureTime": depart, "sessionEndTime": session_end,
                "routeCreationTime": depart - 21_600_000, "routeDuration": 36000,
                "itineraryDuration": day.length * 60, "stopProgress": {"completed": completed, "total": day.stops},
                "transporterTimeAttributes": {"actualDepartureTime": depart, "plannedDepartureTime": depart},
                "breaks": [], "plannedBreaks": [], "inactiveTasks": [], "unknownStops": [], "rescueActions": [],
                "stops": stops, "weatherData": null},
                "addresses": addresses, "transporters": [person.clone()], "companies": companies,
                "itineraryPartners": null, "routePauseStatus": null})
            .to_string(),
        });
        people.push(person);
    }
    (
        (Value::Array(summaries), Value::Array(people)),
        Value::Array(routes),
        itineraries,
    )
}
