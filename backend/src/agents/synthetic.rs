//! A synthetic DSP to try agents against: two weeks of Northline's demo people as every
//! source would hold them. Routes and meal breaks go through the same staging and
//! publishing a collection uses, so they read exactly as collected data does. Only a
//! development server with fixture providers, seeded first, can make it.
use crate::{
    Error, Result,
    db::{Store, s},
    ensure, meals,
    routedata::{self, ItineraryCapture, Mode, Request},
};
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use serde_json::{Value, json};

/// How many days end with yesterday.
pub const DAYS: i64 = 14;
pub const STATION: &str = "DEMO1";

/// A number from a seed, the same on every run.
fn roll(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One driver's day, decided before any source writes it down.
struct Day {
    /// Minutes after the DSP's midnight.
    departed: i64,
    stops: i64,
    missed: i64,
    /// The meal Cortex records, if one was taken: its start, in minutes after midnight.
    meal: Option<i64>,
    /// Whether Paycom has the lunch punched; it may not match Cortex.
    lunch_punched: bool,
    length: i64,
    inspection_seconds: i64,
}
impl Day {
    fn last_stop(&self) -> i64 {
        self.departed + self.length
    }
    fn clock_in(&self) -> i64 {
        self.departed - 40
    }
    fn clock_out(&self) -> i64 {
        self.last_stop() + 55
    }
}

fn plan(driver: u64, day: i64) -> Option<Day> {
    // Two days off a week, in a pattern of each driver's own.
    let off = (day + driver as i64) % 7 == 0 || (day + 2 * driver as i64) % 7 == 3;
    if off {
        return None;
    }
    let r = roll(driver * 1000 + day as u64);
    let stops = 105 + (r % 50) as i64;
    let missed = if (r >> 8).is_multiple_of(4) {
        1 + ((r >> 12) % 3) as i64
    } else {
        0
    };
    let departed = 9 * 60 + 30 + ((r >> 16) % 45) as i64;
    let length = 400 + ((r >> 20) % 120) as i64;
    let meal = (!(r >> 28).is_multiple_of(9)).then_some(departed + length / 2);
    let short = (r >> 40).is_multiple_of(10);
    Some(Day {
        departed,
        stops,
        missed,
        meal,
        lunch_punched: meal.is_some() || (r >> 36).is_multiple_of(2),
        length,
        inspection_seconds: if short {
            30 + ((r >> 44) % 50) as i64
        } else {
            120 + ((r >> 44) % 300) as i64
        },
    })
}

fn hhmm(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Fills Northline with two weeks of routes, timecards, meal breaks and inspections for
/// its demo people, then matches them. Returns what it made.
pub fn seed(db: &Store) -> Result<Value> {
    ensure(
        db.config.development && db.config.fixture,
        "fixtures_require_development",
        403,
    )?;
    let dsp = db
        .platform
        .one(
            "SELECT id,timezone FROM dsps WHERE name='Northline Logistics'",
            [],
        )?
        .ok_or_else(|| Error::new("seed_required", 409))?;
    let (id, timezone) = (s(&dsp, "id").to_owned(), s(&dsp, "timezone").to_owned());
    let zone: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    db.enable_all_features(&id)?;
    db.set_profile(
        &id,
        json!({"stationCode": STATION, "abbreviation": "NLOG", "setupRequired": false}),
    )?;
    db.collector(&id, crate::collectors::Provider::Cortex)?
        .exec(
            "UPDATE connections SET enabled=1,status='ready',account_label='fixture'",
            [],
        )?;
    let today = chrono::Utc::now().with_timezone(&zone).date_naive();
    let dates: Vec<NaiveDate> = (1..=DAYS)
        .rev()
        .map(|back| today - Duration::days(back))
        .collect();
    let at = |date: NaiveDate, minutes: i64| -> i64 {
        zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|midnight| midnight.timestamp_millis() + minutes * 60_000)
            .unwrap_or_default()
    };

    // Paycom: Avery Morgan dispatches; everyone else drives.
    let roster = crate::workforce::fixture(&timezone)?;
    let employees = roster["employees"].as_array().cloned().unwrap_or_default();
    let drivers: Vec<(u64, String, String)> = employees
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, e)| (i as u64, s(e, "code").to_owned(), s(e, "name").to_owned()))
        .collect();
    let transporter = |i: u64| format!("A{:02}SYNTH{:02}", i * 7 % 97, i);
    let dispatcher = employees.first().map(|e| s(e, "code").to_owned());
    let mut timecards = vec![];
    for (d, date) in dates.iter().enumerate() {
        if let Some(code) = &dispatcher
            && date.weekday().number_from_monday() <= 5
        {
            timecards.push(
                json!({"employeeCode": code, "date": date.to_string(), "hours": 8.0,
                "status": "Complete", "punches": [{"in":"08:00","out":"12:00","hours":4},
                {"in":"12:30","out":"16:30","hours":4}]}),
            );
        }
        for (i, code, _) in &drivers {
            let Some(day) = plan(*i, d as i64) else {
                continue;
            };
            let lunch = if day.lunch_punched {
                day.meal.unwrap_or(day.departed + day.length / 2)
            } else {
                -1
            };
            let worked = day.clock_out() - day.clock_in() - if lunch >= 0 { 30 } else { 0 };
            let punches = if lunch >= 0 {
                json!([
                    {"in": hhmm(day.clock_in()), "out": hhmm(lunch),
                     "hours": (lunch - day.clock_in()) as f64 / 60.0},
                    {"in": hhmm(lunch + 30), "out": hhmm(day.clock_out()),
                     "hours": (day.clock_out() - lunch - 30) as f64 / 60.0}
                ])
            } else {
                json!([{"in": hhmm(day.clock_in()), "out": hhmm(day.clock_out()),
                        "hours": worked as f64 / 60.0}])
            };
            timecards.push(json!({"employeeCode": code, "date": date.to_string(),
                "hours": (worked as f64 / 60.0 * 100.0).round() / 100.0,
                "status": "Complete", "punches": punches}));
        }
    }
    db.publish(
        &id,
        &json!({"employees": employees, "timecards": timecards, "collectedAt": crate::db::iso(),
            "from": dates[0].to_string(), "to": dates[dates.len() - 1].to_string()}),
    )?;

    // Routes, a day at a time, through the collection's own staging and publishing.
    let mut routes = 0;
    for (d, date) in dates.iter().enumerate() {
        let day = date.to_string();
        let jobs = db.enqueue_routes(
            &id,
            None,
            &format!("synthetic:{day}"),
            Some(&day),
            Mode::Final,
            1,
        )?;
        let job = s(&jobs[0], "id").to_owned();
        let request: Value = serde_json::from_str(&db.job_row(&job, Some(&id))?.request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        let mut capture = routedata::fixture(&request)?;
        let (summaries, route_summaries, itineraries) =
            routes_day(&capture, *date, d as i64, &drivers, &transporter, &at);
        routes += itineraries.len();
        capture.summaries["itinerarySummaries"] = summaries.0;
        capture.summaries["transporters"] = summaries.1;
        capture.route_summaries["rmsRouteSummaries"] = route_summaries;
        capture.route_summaries["transporters"] = capture.summaries["transporters"].clone();
        capture.itineraries = itineraries;
        let staged = db.stage_routes(&id, &job, capture)?;
        db.publish_routes(&id, &job, &staged)?;
        db.jobs
            .exec("UPDATE jobs SET status='succeeded' WHERE id=?", [&job])?;

        // Cortex's meal breaks for the same itineraries.
        let scope = meals::Scope {
            date: day.clone(),
            station: STATION.into(),
            service_area_id: request
                .service_area_id
                .clone()
                .unwrap_or_else(|| "area-demo".into()),
            provider: request
                .provider
                .clone()
                .unwrap_or_else(|| "provider-demo".into()),
            timezone: timezone.clone(),
        };
        let observed = crate::db::now();
        let mut found = vec![];
        for (i, _, name) in &drivers {
            let Some(plan) = plan(*i, d as i64) else {
                continue;
            };
            found.push(meals::Itinerary {
                id: format!("synthetic-{day}-{i}"),
                transporter_id: transporter(*i),
                driver: name.clone(),
                route: format!("CX{}", 100 + i),
                observed_at: observed,
                route_complete: true,
                delivery_coverage: meals::Coverage::Complete,
                meals: plan
                    .meal
                    .map(|start| meals::Meal {
                        id: format!("meal-{day}-{i}"),
                        start: at(*date, start),
                        end: Some(at(*date, start + 30)),
                        last_delivery: Some(at(*date, start - 4)),
                        first_delivery: Some(at(*date, start + 36)),
                        last_delivery_stop: None,
                        first_delivery_stop: None,
                    })
                    .into_iter()
                    .collect(),
                source_url: None,
            });
        }
        db.publish_meals(
            &id,
            &format!("synthetic-meals:{day}"),
            &meals::Capture {
                scope: scope.clone(),
                started_at: observed,
                finished_at: observed,
                itineraries: found,
            },
            &scope,
        )?;
    }

    // DVIC: one pre-trip inspection each route day; under 90 seconds is short.
    let dvic = db.dvic(&id)?;
    let (first, last) = (dates[0].to_string(), dates[dates.len() - 1].to_string());
    dvic.exec(
        "INSERT OR REPLACE INTO dvic_reports(id,company_id,dsp_code,station,source_key,name,week,\
         report_date,modified_at,sha256,revision_id,row_count,short_count,min_date,max_date,checked_at) \
         VALUES ('synthetic','synthetic','NLOG',?1,'synthetic','DVIC','synthetic',?3,0,'synthetic',\
         'synthetic',0,0,?2,?3,?3)",
        rusqlite::params![STATION, first, last],
    )?;
    dvic.exec(
        "INSERT OR REPLACE INTO dvic_revisions(id,report_id,sha256,modified_at,collected_at,rows) \
         VALUES ('synthetic','synthetic','synthetic',0,?,'[]')",
        [&last],
    )?;
    let mut inspections = 0;
    for (d, date) in dates.iter().enumerate() {
        for (i, _, name) in &drivers {
            let Some(plan) = plan(*i, d as i64) else {
                continue;
            };
            dvic.exec(
                "INSERT OR REPLACE INTO dvic_inspections(company_id,inspection_key,dsp_code,station,\
                 start_date,transporter_id,transporter_name,vin,fleet_type,inspection_type,\
                 inspection_status,start_time,end_time,duration_seconds,minimum_seconds,short,\
                 report_date,source_modified_at,revision_id) VALUES ('synthetic',?1,'NLOG',?2,?3,?4,?5,\
                 ?6,'CDV','PRE_TRIP','COMPLETE',?7,?8,?9,90,?10,?3,0,'synthetic')",
                rusqlite::params![
                    format!("synthetic-{date}-{i}"),
                    STATION,
                    date.to_string(),
                    transporter(*i),
                    name,
                    format!("SYNTHVIN{i:09}"),
                    hhmm(plan.departed - 15),
                    hhmm(plan.departed - 15 + (plan.inspection_seconds + 59) / 60),
                    plan.inspection_seconds as f64,
                    i64::from(plan.inspection_seconds < 90),
                ],
            )?;
            inspections += 1;
        }
    }
    drop(dvic);
    let matched = db.match_drivers(&id)?;
    Ok(
        json!({"dsp": id, "from": first, "to": last, "timecards": timecards.len(),
        "routes": routes, "inspections": inspections, "matched": matched}),
    )
}

type Summaries = (Value, Value);
/// One day's lists and itineraries, in the shape Cortex answers with.
fn routes_day(
    capture: &routedata::Capture,
    date: NaiveDate,
    d: i64,
    drivers: &[(u64, String, String)],
    transporter: &dyn Fn(u64) -> String,
    at: &dyn Fn(NaiveDate, i64) -> i64,
) -> (Summaries, Value, Vec<ItineraryCapture>) {
    let provider = capture.scope.provider.clone();
    let area = capture.scope.service_area_id.clone();
    let companies = json!([{"companyId": provider, "companyName": "Synthetic Logistics",
        "companyShortCode": "SYN", "id": provider}]);
    let (mut summaries, mut people, mut routes, mut itineraries) = (vec![], vec![], vec![], vec![]);
    for (i, _, name) in drivers {
        let Some(day) = plan(*i, d) else {
            continue;
        };
        let (first, last) = name.split_once(' ').unwrap_or((name, ""));
        let id = format!("synthetic-{date}-{i}");
        let route = format!("CX{}", 100 + i);
        let who = transporter(*i);
        let depart = at(date, day.departed);
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
            let count = 1 + (roll((*i << 32) + (d << 16) as u64 + n as u64) % 2) as i64;
            let state = if missed { "UNDELIVERABLE" } else { "DELIVERED" };
            let tasks: Vec<Value> = (0..count)
                .map(|k| {
                    let tracking = format!("TBA{:04}{:02}{:03}{}", date.ordinal(), i, n, k);
                    json!({"taskId": format!("{id}-{n}-{k}"), "transporterId": null,
                        "referenceId": format!("{id}-{n}-{k}"), "taskType": "DROP_OFF", "taskState": state,
                        "taskStateContext": if missed {"BUSINESS_CLOSED"} else {"DELIVERED_TO_DOORSTEP"},
                        "executionStatus": "COMPLETE", "addressId": format!("{id}-a{n}"),
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
            addresses.push(json!({"addressId": format!("{id}-a{n}"), "address1": format!("{} Synthetic Ave", 100 + n),
                "address2": null, "address3": null, "city": "Example", "state": "IL", "postalCode": "60000",
                "geocode": {"latitude": 41.8, "longitude": -87.6, "scope": 0}}));
            stops.push(json!({"stopId": format!("{id}-{n}"), "sequenceNumber": n, "addressId": format!("{id}-a{n}"),
                "itineraryStopType": "DROP_OFF", "routeCode": route, "plannedStartTime": when,
                "plannedEndTime": when + 300_000, "expectedStartTime": when, "actualStartTime": when,
                "highValueStopFlag": false, "tasks": tasks}));
        }
        let total = packages.0 + packages.1;
        let completed = day.stops - day.missed;
        let session_end = at(date, day.last_stop() + 40);
        let breaks = if day.meal.is_some() { 1800 } else { 0 };
        let person = json!({"transporterId": who, "firstName": first, "lastName": last, "initials": "SD",
            "workPhoneNumber": "15550000000", "personType": ["DELIVERY_ASSOCIATE"], "lastLocation": null});
        summaries.push(json!({"itineraryId": id, "transporterId": who, "companyId": provider, "routeCode": route,
            "routeCodes": [route], "vinNumber": format!("SYNTHVIN{i:09}"), "executionStatus": "COMPLETE",
            "progressStatus": "ON_TIME", "serviceTypeName": "Standard Parcel", "plannedDepartureTime": depart,
            "itineraryStartTime": depart, "waveStartTime": depart - 1_800_000, "sessionEndTime": session_end,
            "projectedCompletionTime": session_end, "lastStopExecutionTime": at(date, day.last_stop()),
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
