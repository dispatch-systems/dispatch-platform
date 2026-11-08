//! Collection requests, queue scheduling, inactive staging and publication commands.
use super::RoutesStore;
use crate::backend::{
    ADAPTER_VERSION, BACKFILL_DAYS, MAX_DAYS_PER_REQUEST, MAX_JOBS_PER_RUN, changed,
    publishing::{Writes, publish_day, write_itinerary, write_raw, write_routes, write_shared},
    scope::{day_allowed, default_day, retention_start, routes_request, routes_today},
    shaping::{PreparedItinerary, PreparedResponse, Shaper, prepare_lists},
    staging::Staged,
    work_phones,
};
use chrono::{Duration, NaiveDate};
use dispatch_core::{
    Error, Result,
    db::{Store, at},
    ensure,
};
use dispatch_cortex::{
    self as cortex,
    routes::{Capture, Collection, Mode, Request},
};
use rusqlite::params;
use serde_json::Value;

/// Queues `days` days ending at `day`, or the default day of `mode`, answering with
/// the jobs. A day already queued under `key` answers the same job.
pub(super) fn enqueue_routes(
    store: &Store,
    id: &str,
    actor: Option<&str>,
    key: &str,
    day: Option<&str>,
    mode: Mode,
    days: i64,
) -> Result<Value> {
    ensure(
        (1..=MAX_DAYS_PER_REQUEST).contains(&days),
        "invalid_input",
        400,
    )?;
    let today = routes_today(store, id)?;
    let last = day
        .map(str::to_owned)
        .unwrap_or_else(|| default_day(mode, today));
    day_allowed(mode, &last, today)?;
    ensure(days == 1 || mode == Mode::Final, "invalid_input", 400)?;
    // A day the retention window has passed would be deleted again within the hour.
    if let Some(start) = retention_start(store, id)? {
        let first = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
            .map_err(|_| Error::new("invalid_date", 400))?
            - Duration::days(days - 1);
        ensure(first >= start, "routes_day_outside_retention", 400)?;
    }
    let last_date = NaiveDate::parse_from_str(&last, "%Y-%m-%d")
        .map_err(|_| Error::new("invalid_date", 400))?;
    let requests = (0..days)
        .map(|back| {
            let day = (last_date - Duration::days(back)).to_string();
            let suffix = if days == 1 {
                String::new()
            } else {
                format!(":{day}")
            };
            Ok((
                format!("{key}{suffix}"),
                cortex::PROVIDER,
                routes_request(store, id, &day, mode)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let jobs = store.enqueue_batch(id, actor, &requests)?;
    Ok(serde_json::to_value(jobs)?)
}

pub(super) fn routes_schedule_ready(store: &Store, id: &str) -> Result<()> {
    ensure(
        !store.profile(id)?.station_code.is_empty(),
        "routes_station_required",
        409,
    )
}

/// The days a scheduled run collects in `mode`: a snapshot of today; or as final,
/// yesterday and recent days without a final publication, newest first. A snapshot of
/// a day leaves its final read to the first run after it ends.
pub(super) fn routes_jobs(store: &Store, id: &str, mode: Mode) -> Result<Vec<(String, Value)>> {
    let today = routes_today(store, id)?;
    if mode == Mode::Snapshot {
        let day = today.to_string();
        return Ok(vec![(
            format!("routes:{day}"),
            routes_request(store, id, &day, mode)?,
        )]);
    }
    let station = store.profile(id)?.station_code;
    let db = store.routes_db(id)?;
    let mut jobs = Vec::new();
    for back in 1..=(BACKFILL_DAYS as i64) {
        let day = (today - Duration::days(back)).to_string();
        let published = db.one(
            "SELECT id FROM route_publications WHERE day=? AND station=? AND mode='final' AND active=1",
            [&day, &station],
        )?;
        if published.is_none() {
            jobs.push((
                format!("routes:{day}"),
                routes_request(store, id, &day, Mode::Final)?,
            ));
            if jobs.len() >= MAX_JOBS_PER_RUN {
                break;
            }
        }
    }
    Ok(jobs)
}

/// Starts a day's inactive publication for `job`: the publication row, both lists'
/// responses and the planned routes. A previous attempt's publication is left to the
/// sweep.
/// `lists` is the validated capture with its itineraries taken out; `itineraries`
/// is how many it had.
pub(super) fn stage_routes_start(
    store: &Store,
    id: &str,
    job: &str,
    lists: &Capture,
    itineraries: usize,
) -> Result<Staged> {
    let responses = prepare_lists(lists)?;
    stage_prepared_routes_start(store, id, job, lists, itineraries, &responses)
}

/// Starts a runtime publication from responses prepared before entering the platform write.
pub(in crate::backend) fn stage_prepared_routes_start(
    store: &Store,
    id: &str,
    job: &str,
    lists: &Capture,
    itineraries: usize,
    responses: &[PreparedResponse],
) -> Result<Staged> {
    let capture = lists;
    let db = store.routes_db(id)?;
    let publication = dispatch_core::foundation::crypto::id("routes")?;
    let routes = capture.route_summaries["rmsRouteSummaries"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let day = capture.scope.date.as_str();
    db.transaction(|| {
        // A previous attempt's day is handed to `sweep_routes`, which deletes it in
        // small steps; its job id is freed for this attempt.
        db.exec(
            "UPDATE route_publications SET job_id=job_id||':abandoned:'||id \
             WHERE job_id=? AND active=0",
            [job],
        )?;
        db.0.prepare_cached(
            "INSERT INTO route_publications(id,job_id,day,station,service_area_id,provider,timezone,mode,\
             started_at,collected_at,active,route_count,itinerary_count,stop_count,task_count,adapter_version) \
             VALUES (?,?,?,?,?,?,?,?,?,?,0,?,?,0,0,?)",
        )?
        .execute(params![
            publication,
            job,
            day,
            capture.scope.station,
            capture.scope.service_area_id,
            capture.scope.provider,
            capture.scope.timezone,
            capture.mode.as_str(),
            at(capture.started_at),
            at(capture.finished_at),
            routes.len() as i64,
            itineraries as i64,
            ADAPTER_VERSION
        ])?;
        write_routes(&db, Writes::Staged, &publication, day, &routes)?;
        for response in responses {
            write_raw(&db, Writes::Staged, &publication, response.name, response.bytes, &response.body)?;
        }
        Ok(())
    })?;
    Ok(Staged {
        collection: Collection::Routes,
        publication,
        day: day.into(),
        station: capture.scope.station.clone(),
        service_area_id: capture.scope.service_area_id.clone(),
        provider: capture.scope.provider.clone(),
        itineraries,
    })
}

/// Stages a whole capture at once, without the platform's lock: what tests and
/// tools use where `stage` would need a running job.
pub(super) fn stage_routes(
    store: &Store,
    id: &str,
    job: &str,
    mut capture: Capture,
) -> Result<Staged> {
    let request: Value = serde_json::from_str(&store.job_row(job, Some(id))?.request)?;
    let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
    capture.validate(&request)?;
    capture.validate_response_budget()?;
    let shaper = Shaper::new(&capture);
    let itineraries = std::mem::take(&mut capture.itineraries);
    let staged = store.stage_routes_start(id, job, &capture, itineraries.len())?;
    for captured in &itineraries {
        store.stage_routes_itinerary(id, &staged, &shaper.shape(captured)?)?;
    }
    Ok(staged)
}

/// Adds one shaped itinerary to a staged publication, in one short transaction.
pub(super) fn stage_routes_itinerary(
    store: &Store,
    id: &str,
    staged: &Staged,
    itinerary: &PreparedItinerary,
) -> Result<()> {
    let db = store.routes_db(id)?;
    db.transaction(|| {
        ensure(
            db.one(
                "SELECT id FROM route_publications WHERE id=? AND active=0",
                [&staged.publication],
            )?
            .is_some(),
            "routes_capture_invalid",
            502,
        )?;
        write_itinerary(&db, Writes::Staged, &staged.publication, &staged.day, itinerary)?;
        write_shared(&db, Writes::Staged, &staged.publication, &staged.day, itinerary)?;
        write_raw(
            &db,
            Writes::Staged,
            &staged.publication,
            &format!("itinerary:{}", itinerary.id),
            itinerary.raw_bytes,
            &itinerary.raw,
        )?;
        db.exec(
            "UPDATE route_publications SET stop_count=stop_count+?,task_count=task_count+? WHERE id=?",
            params![
                itinerary.stops.len() as i64,
                itinerary.tasks.len() as i64,
                staged.publication
            ],
        )?;
        Ok(())
    })
}

/// Makes a staged day the day's read if it was collected later than the one the day has
/// (`publish_day`). A day it changed is noted, to tell of (`changed`). Answers whether it is
/// the day's read now.
pub(super) fn publish_routes(store: &Store, id: &str, job: &str, staged: &Staged) -> Result<bool> {
    let routes = &*store.routes_db(id)?;
    routes.transaction(|| {
        let current = publish_day(routes, job, staged, work_phones::Stored::default())?;
        if current {
            changed::note(routes, &staged.day)?;
        }
        Ok(current)
    })
}
