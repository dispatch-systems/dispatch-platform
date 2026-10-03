//! Daily routes from Cortex's execution pages: the station's route list, every driver's
//! itinerary with its stops and packages, and the newer routes page's list, one day at a
//! time, published into the DSP's route data database. Normalized rows hold what reads
//! filter on; every response is kept whole, compressed, for reprocessing. Browser and
//! HTTP data is untrusted input. The collection is `routes` to the platform; the module
//! and its database are `routedata`, since `routes` names the HTTP routes here.
#[path = "keeper.rs"]
pub mod keeper;
#[path = "maintenance.rs"]
pub mod maintenance;
#[path = "people.rs"]
pub mod people;

use crate::{
    Error, Result,
    collectors::{
        AddedStorage,
        cortex::{
            self,
            discovery::Scope,
            routes::{
                Capture, Collection, ItineraryCapture, JOB_KIND, MAX_BODY, MAX_CAPTURE_BYTES, Mode,
                Request, add_capture_bytes, listed,
            },
        },
    },
    contracts::{
        RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
        RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteRetention,
        RouteStop, RouteTask, RouteUnknownStop,
    },
    db::{Db, DspLease, Kind, Store, at, migrations::add_column, s},
    ensure,
    read_cache::DataDomain,
};
use chrono::{Duration, NaiveDate};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::Write;

pub const COLLECTION: &str = "routes";
/// The routes, itineraries and packages it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("routes");
/// What shaped a publication's rows. 2 keys tasks by itinerary and keeps their events
/// as triples; a reprocess brings an older publication up to date.
pub const ADAPTER_VERSION: i64 = 2;
/// The route data database beside `cortex.sqlite`. It holds a day of tasks per
/// publication and answers day-range reads, so its indexes stay in memory.
pub const DATABASE: Kind = Kind::new("routedata", 1).cache(32768);
pub static STORAGE: AddedStorage = AddedStorage {
    id: "routedata",
    kind: DATABASE,
    marker: "storage.routedata",
    source: "routedata-v1",
    verify,
};
/// Migration 2: removed tasks join `tasks`, marked inactive; the itinerary keeps its
/// route-level lists; breaks and unknown stops get tables, and two views pre-join the
/// common questions.
pub(crate) fn add_details(db: &Db) -> Result<()> {
    add_column(db, "tasks", "active", "INTEGER NOT NULL DEFAULT 1")?;
    for column in ["rescue_actions", "sequence_edits", "pause_events"] {
        add_column(db, "itineraries", column, "TEXT")?;
    }
    db.0.execute_batch(include_str!("../migrations/routedata/0002_details.sql"))?;
    Ok(())
}
/// How far back a schedule collects days it has no final publication of.
pub const BACKFILL_DAYS: usize = 7;
/// Jobs one scheduled run queues at most; the queue admits five active per DSP.
pub const MAX_JOBS_PER_RUN: usize = 4;
/// Days one request may queue at once.
pub const MAX_DAYS_PER_REQUEST: i64 = 5;
/// The shortest and longest retention a DSP may choose, in days. Longer than the
/// schedule's backfill, so a window never deletes a day a schedule would collect again.
pub const MIN_RETENTION_DAYS: i64 = 30;
pub const MAX_RETENTION_DAYS: i64 = 3650;

/// One itinerary's rows, shaped and compressed ahead of its insert.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedItinerary {
    id: String,
    transporter_id: String,
    columns: Vec<(String, Value)>,
    stops: Vec<StopRow>,
    tasks: Vec<TaskRow>,
    /// Tasks Amazon removed from the route, kept for the record.
    inactive: Vec<TaskRow>,
    breaks: Vec<BreakRow>,
    unknown_stops: Vec<UnknownStopRow>,
    day: DayRow,
    addresses: Vec<Value>,
    person: Value,
    raw_bytes: usize,
    /// The detail gzip-compressed.
    #[serde(skip)]
    raw: Vec<u8>,
}
/// A day stored as an inactive publication, one itinerary at a time, waiting for its
/// job to make it the day's active one.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Staged {
    pub collection: Collection,
    pub publication: String,
    pub day: String,
    pub station: String,
    pub service_area_id: String,
    pub provider: String,
    pub itineraries: usize,
}
/// An identifier a row can carry: printable text of a bounded length. Amazon's task and
/// stop ids mix letters, digits and punctuation.
pub fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.chars().any(|c| c.is_control())
        && !value.trim().is_empty()
}
/// An epoch time as the API sends it: milliseconds, or seconds for older fields.
pub fn stamp(value: &Value) -> Option<i64> {
    let number = value.as_f64()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    Some(if number < 100_000_000_000.0 {
        (number * 1000.0).round() as i64
    } else {
        number.round() as i64
    })
}
fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.chars().take(512).collect()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
fn integer(value: &Value) -> Option<i64> {
    number(value).map(|n| n.round() as i64)
}
fn flag(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
}
/// The names of an object's flags that are set, for a compact record of a stop's kinds.
fn set_flags(value: &Value) -> Value {
    let mut names: Vec<&str> = value
        .as_object()
        .into_iter()
        .flat_map(|o| o.iter())
        .filter(|(_, v)| **v == json!(true))
        .map(|(k, _)| k.as_str())
        .collect();
    names.sort_unstable();
    json!(names)
}
fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}
/// A stored response, inflated.
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut out)?;
    ensure(out.len() <= MAX_BODY, "routes_source_too_large", 502)?;
    Ok(out)
}

/// What shaping an itinerary needs from the day's list: each itinerary's summary and
/// the drivers it names.
pub struct Shaper {
    summaries: Map<String, Value>,
    people: Vec<Value>,
}
impl Shaper {
    pub fn new(capture: &Capture) -> Self {
        Self {
            summaries: capture.summaries["itinerarySummaries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| Some((v["itineraryId"].as_str()?.to_owned(), v.clone())))
                .collect(),
            people: capture.summaries["transporters"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        }
    }
    /// One itinerary's rows and its compressed response. Its parsed tree lives only
    /// while this runs.
    pub fn shape(&self, captured: &ItineraryCapture) -> Result<PreparedItinerary> {
        let parsed = captured.parsed()?;
        let detail = &parsed["itineraryDetails"];
        let summary = self
            .summaries
            .get(&captured.id)
            .cloned()
            .unwrap_or(Value::Null);
        let person = self
            .people
            .iter()
            .chain(parsed["transporters"].as_array().into_iter().flatten())
            .find(|t| t["transporterId"] == json!(captured.transporter_id))
            .cloned()
            .unwrap_or(Value::Null);
        let driver_name = match (text(&person["firstName"]), text(&person["lastName"])) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            _ => {
                text(&detail["transporterName"]).unwrap_or_else(|| captured.transporter_id.clone())
            }
        };
        let flat = flatten(&summary, detail, &captured.transporter_id, &driver_name);
        let (stops, tasks, day) = rows(detail, &captured.transporter_id);
        Ok(PreparedItinerary {
            id: captured.id.clone(),
            transporter_id: captured.transporter_id.clone(),
            columns: flat
                .itinerary
                .into_iter()
                .map(|(c, v)| (c.to_owned(), v))
                .collect(),
            stops,
            tasks,
            inactive: inactive_tasks(detail),
            breaks: break_rows(&summary, detail),
            unknown_stops: unknown_stop_rows(detail),
            day: DayRow {
                departed_at: stamp(&detail["transporterTimeAttributes"]["actualDepartureTime"])
                    .or_else(|| stamp(&summary["itineraryStartTime"]))
                    .or_else(|| stamp(&detail["itineraryStartTime"])),
                session_end_at: stamp(&summary["sessionEndTime"])
                    .or_else(|| stamp(&detail["sessionEndTime"])),
                route_code: text(&summary["routeCode"]).or_else(|| text(&detail["routeCode"])),
                breaks_secs: integer(&summary["totalBreaksDurationSecs"]),
                overtime_secs: integer(&summary["overtimeDurationSecs"])
                    .or_else(|| integer(&detail["overtimeDurationSecs"])),
                ..day
            },
            addresses: parsed["addresses"].as_array().cloned().unwrap_or_default(),
            person,
            raw_bytes: captured.detail.len(),
            raw: gzip(captured.detail.as_bytes())?,
        })
    }
}
/// Every itinerary shaped: what a reprocess, which rebuilds a stored day at once, needs.
pub fn prepare(capture: &Capture) -> Result<Vec<PreparedItinerary>> {
    // Reject the aggregate before parsing any itinerary into a larger JSON tree.
    capture.validate_response_budget()?;
    let shaper = Shaper::new(capture);
    capture
        .itineraries
        .iter()
        .map(|i| shaper.shape(i))
        .collect()
}

/// Stores a finished day as an inactive publication, one itinerary at a time. Checking
/// and shaping happen on blocking threads without the platform lock; each insert takes
/// it only for that itinerary's rows, and first checks the job is still this worker's.
/// Only one itinerary's parsed tree and rows exist at once.
pub async fn stage(
    state: &std::sync::Arc<crate::State>,
    dsp: &str,
    job: &str,
    owner: &str,
    mut capture: Capture,
) -> Result<Staged> {
    let blocking = |error: tokio::task::JoinError| {
        crate::observability::event(
            "error",
            "routes.stage_failed",
            json!({"panic":error.is_panic()}),
        );
        Error::new("routes_capture_invalid", 500)
    };
    let (d, j) = (dsp.to_owned(), job.to_owned());
    let request = state
        .read(move |db| Ok(db.job_row(&j, Some(&d))?.request))
        .await?;
    let request = Request::parse(&serde_json::from_str(&request)?)?
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let capture_checked = tokio::task::spawn_blocking(move || {
        capture.validate(&request)?;
        capture.validate_response_budget()?;
        Ok::<_, Error>(capture)
    })
    .await
    .map_err(blocking)??;
    capture = capture_checked;
    let shaper = std::sync::Arc::new(Shaper::new(&capture));
    let itineraries = std::mem::take(&mut capture.itineraries);
    let count = itineraries.len();
    let (d, j, o) = (dsp.to_owned(), job.to_owned(), owner.to_owned());
    let staged = state
        .run_scoped(dsp, DOMAIN, move |db| {
            db.guard(&j, &o)?;
            db.stage_routes_start(&d, &j, &capture, count)
        })
        .await?;
    for captured in itineraries {
        let shaper = shaper.clone();
        let shaped = tokio::task::spawn_blocking(move || shaper.shape(&captured))
            .await
            .map_err(blocking)??;
        let (d, j, o, staged) = (
            dsp.to_owned(),
            job.to_owned(),
            owner.to_owned(),
            staged.clone(),
        );
        state
            .run_scoped(dsp, DOMAIN, move |db| {
                db.guard(&j, &o)?;
                db.stage_routes_itinerary(&d, &staged, &shaped)
            })
            .await?;
    }
    Ok(staged)
}

fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM routedata_schema", [])? == vec![json!({"version":1})],
        "unsupported_routedata_schema",
        503,
    )?;
    // Missing initialized tables fail closed, rather than recreating lost data.
    for table in [
        "route_publications",
        "routes",
        "itineraries",
        "stops",
        "tasks",
        "addresses",
        "drivers",
        "driver_days",
        "route_raw",
    ] {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// The day `mode` reads by default: yesterday for a final read, today for a snapshot.
pub fn default_day(mode: Mode, today: NaiveDate) -> String {
    match mode {
        Mode::Final => (today - Duration::days(1)).to_string(),
        Mode::Snapshot => today.to_string(),
    }
}
/// Whether `day` can be read in `mode` on `today`: a final read needs a completed day,
/// a snapshot reads today only.
pub fn day_allowed(mode: Mode, day: &str, today: NaiveDate) -> Result<()> {
    let date =
        NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Error::new("invalid_date", 400))?;
    ensure(date.to_string() == day, "invalid_date", 400)?;
    ensure(
        match mode {
            Mode::Final => date < today,
            Mode::Snapshot => date == today,
        },
        "routes_day_not_available",
        400,
    )
}

/// A driver's itinerary flattened for its row.
struct Flat {
    itinerary: Vec<(&'static str, Value)>,
}

/// What Routes reads and writes for a DSP: each day's routes, itineraries and packages, in
/// the database beside Cortex's, the collections that bring them and how long they are kept.
pub trait RoutesStore {
    fn routes_db(&self, id: &str) -> Result<DspLease<'_>>;
    fn enqueue_routes(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        day: Option<&str>,
        mode: Mode,
        days: i64,
    ) -> Result<Value>;
    fn routes_schedule_ready(&self, id: &str) -> Result<()>;
    fn routes_jobs(&self, id: &str) -> Result<Vec<(String, Value)>>;
    fn stage_routes_start(
        &self,
        id: &str,
        job: &str,
        lists: &Capture,
        itineraries: usize,
    ) -> Result<Staged>;
    fn stage_routes(&self, id: &str, job: &str, capture: Capture) -> Result<Staged>;
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()>;
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<()>;
    fn route_retention(&self, id: &str) -> Result<RouteRetention>;
    fn set_route_retention(
        &self,
        id: &str,
        actor: Option<&str>,
        days: Option<i64>,
    ) -> Result<RouteRetention>;
    fn expire_routes(&self, id: &str) -> Result<usize>;
    fn sweep_routes(&self, id: &str) -> Result<bool>;
    fn reprocess_routes(&self, id: &str, day: Option<&str>) -> Result<RouteReprocess>;
    fn route_days(&self, id: &str) -> Result<RouteDays>;
    fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>>;
    fn route_itinerary(
        &self,
        id: &str,
        day: &str,
        itinerary_id: &str,
    ) -> Result<Option<RouteItineraryDetail>>;
    fn route_package(&self, id: &str, tracking: &str) -> Result<RoutePackage>;
}
impl RoutesStore for Store {
    fn routes_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &STORAGE)
    }
    /// Queues `days` days ending at `day`, or the default day of `mode`, answering with
    /// the jobs. A day already queued under `key` answers the same job.
    fn enqueue_routes(
        &self,
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
        let today = routes_today(self, id)?;
        let last = day
            .map(str::to_owned)
            .unwrap_or_else(|| default_day(mode, today));
        day_allowed(mode, &last, today)?;
        ensure(days == 1 || mode == Mode::Final, "invalid_input", 400)?;
        // A day the retention window has passed would be deleted again within the hour.
        if let Some(start) = retention_start(self, id)? {
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
                    routes_request(self, id, &day, mode)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let jobs = self.enqueue_batch(id, actor, &requests)?;
        Ok(serde_json::to_value(jobs)?)
    }
    fn routes_schedule_ready(&self, id: &str) -> Result<()> {
        ensure(
            !self.profile(id)?.station_code.is_empty(),
            "routes_station_required",
            409,
        )
    }
    /// The days a scheduled run collects: yesterday, and recent days without a final
    /// publication, newest first.
    fn routes_jobs(&self, id: &str) -> Result<Vec<(String, Value)>> {
        let today = routes_today(self, id)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
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
                    routes_request(self, id, &day, Mode::Final)?,
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
    fn stage_routes_start(
        &self,
        id: &str,
        job: &str,
        lists: &Capture,
        itineraries: usize,
    ) -> Result<Staged> {
        let capture = lists;
        let db = self.routes_db(id)?;
        let publication = crate::crypto::id("routes")?;
        let routes = capture.route_summaries["rmsRouteSummaries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let day = capture.scope.date.as_str();
        let summaries = serde_json::to_vec(&capture.summaries)?;
        let route_summaries = serde_json::to_vec(&capture.route_summaries)?;
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
            insert_routes(&db, &publication, day, &routes)?;
            insert_raw(&db, &publication, "summaries", summaries.len(), &gzip(&summaries)?)?;
            insert_raw(
                &db,
                &publication,
                "route_summaries",
                route_summaries.len(),
                &gzip(&route_summaries)?,
            )
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
    fn stage_routes(&self, id: &str, job: &str, mut capture: Capture) -> Result<Staged> {
        let request: Value = serde_json::from_str(&self.job_row(job, Some(id))?.request)?;
        let request = Request::parse(&request)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        capture.validate(&request)?;
        capture.validate_response_budget()?;
        let shaper = Shaper::new(&capture);
        let itineraries = std::mem::take(&mut capture.itineraries);
        let staged = self.stage_routes_start(id, job, &capture, itineraries.len())?;
        for captured in &itineraries {
            self.stage_routes_itinerary(id, &staged, &shaper.shape(captured)?)?;
        }
        Ok(staged)
    }
    /// Adds one shaped itinerary to a staged publication, in one short transaction.
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()> {
        let db = self.routes_db(id)?;
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
            insert_itinerary(&db, &staged.publication, &staged.day, itinerary)?;
            insert_raw(
                &db,
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
    /// Makes a staged day its scope's active publication. Only flags change here, so the
    /// platform lock is held for moments; the day's previous publication is left
    /// inactive, and `sweep_routes` deletes it and its rows in small steps.
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<()> {
        let db = self.routes_db(id)?;
        db.transaction(|| {
            let row = db
                .one(
                    "SELECT itinerary_count,(SELECT count(*) FROM itineraries WHERE publication_id=p.id) stored \
                     FROM route_publications p WHERE id=? AND job_id=? AND active=0",
                    [&staged.publication, job],
                )?
                .ok_or_else(|| Error::new("routes_capture_invalid", 502))?;
            ensure(
                row["itinerary_count"] == row["stored"]
                    && row["stored"] == json!(staged.itineraries),
                "routes_capture_invalid",
                502,
            )?;
            db.exec(
                "UPDATE route_publications SET active=0 WHERE day=? AND station=? AND service_area_id=? \
                 AND provider=? AND active=1",
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
            Ok(())
        })
    }
    /// The DSP's retention window and what its route data holds.
    fn route_retention(&self, id: &str) -> Result<RouteRetention> {
        let db = self.routes_db(id)?;
        let setting = db.one("SELECT days,changed_at FROM route_retention WHERE id=1", [])?;
        let stored = db
            .one(
                "SELECT count(*) days,min(day) oldest FROM route_publications WHERE active=1",
                [],
            )?
            .unwrap_or_default();
        Ok(RouteRetention {
            days: setting.as_ref().and_then(|row| row["days"].as_i64()),
            changed_at: setting
                .as_ref()
                .and_then(|row| row["changed_at"].as_str().map(str::to_owned)),
            stored_days: stored["days"].as_i64().unwrap_or(0),
            oldest_day: stored["oldest"].as_str().map(str::to_owned),
        })
    }
    /// Sets how many days of route data the DSP keeps; `None` keeps every day. A window
    /// takes effect at the next hourly expiry, not in this request.
    fn set_route_retention(
        &self,
        id: &str,
        actor: Option<&str>,
        days: Option<i64>,
    ) -> Result<RouteRetention> {
        if let Some(days) = days {
            ensure(
                (MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&days),
                "invalid_retention",
                400,
            )?;
        }
        let db = self.routes_db(id)?;
        db.exec(
            "INSERT INTO route_retention(id,days,changed_by,changed_at) VALUES (1,?,?,?) \
             ON CONFLICT(id) DO UPDATE SET days=excluded.days,changed_by=excluded.changed_by,\
             changed_at=excluded.changed_at",
            params![days, actor, crate::db::iso()],
        )?;
        self.audit(
            actor,
            Some(id),
            "routes.retention_changed",
            &days.map_or_else(|| "forever".to_owned(), |d| d.to_string()),
        )?;
        self.route_retention(id)
    }
    /// Retires the days a DSP's retention window has passed: they become inactive, and
    /// `sweep_routes` deletes them in small steps. Addresses and drivers last seen before
    /// the window go with them. Nothing is retired while the routes feature is off, so
    /// switching it off never deletes. Answers how many days were retired.
    fn expire_routes(&self, id: &str) -> Result<usize> {
        if !self.feature_enabled(id, COLLECTION)? {
            return Ok(0);
        }
        let Some(start) = retention_start(self, id)? else {
            return Ok(0);
        };
        let start = start.to_string();
        let db = self.routes_db(id)?;
        let retired = db.transaction(|| {
            let retired = db.exec(
                "UPDATE route_publications SET active=0 WHERE active=1 AND day<?",
                [&start],
            )?;
            db.exec("DELETE FROM addresses WHERE last_seen_day<?", [&start])?;
            db.exec("DELETE FROM drivers WHERE last_seen_day<?", [&start])?;
            Ok(retired)
        })?;
        if retired > 0 {
            self.audit(None, Some(id), "routes.data_expired", &start)?;
        }
        Ok(retired)
    }
    /// One small step of removing what no reader sees: publications a newer one
    /// replaced, a retention window retired, or a job left unfinished. Answers whether
    /// more remains. A publication whose job still runs is being staged and is kept.
    fn sweep_routes(&self, id: &str) -> Result<bool> {
        sweep_routes_step(self, id, &mut None)
    }
    /// Rebuilds every active publication's rows, or one day's, from the responses it
    /// stored: what a release that reads more of them needs, without collecting again.
    fn reprocess_routes(&self, id: &str, day: Option<&str>) -> Result<RouteReprocess> {
        if let Some(day) = day {
            crate::validate::date(day)?;
        }
        let db = self.routes_db(id)?;
        let publications = db.all(
            "SELECT id,day,mode,station,service_area_id,provider,timezone,started_at,collected_at \
             FROM route_publications WHERE active=1 AND (?1 IS NULL OR day=?1) ORDER BY day",
            [day],
        )?;
        let mut rebuilt = Vec::new();
        for row in &publications {
            let publication = s(row, "id").to_owned();
            let capture = stored_capture(&db, row)?;
            let prepared = prepare(&capture)?;
            let routes = capture.route_summaries["rmsRouteSummaries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            db.transaction(|| {
                for table in [
                    "routes",
                    "itineraries",
                    "stops",
                    "tasks",
                    "driver_days",
                    "breaks",
                    "unknown_stops",
                ] {
                    db.exec(
                        &format!("DELETE FROM {table} WHERE publication_id=?"),
                        [&publication],
                    )?;
                }
                let day = s(row, "day");
                insert_routes(&db, &publication, day, &routes)?;
                for itinerary in &prepared {
                    insert_itinerary(&db, &publication, day, itinerary)?;
                }
                db.exec(
                    "UPDATE route_publications SET route_count=?,itinerary_count=?,stop_count=?,task_count=?,\
                     adapter_version=? WHERE id=?",
                    params![
                        routes.len() as i64,
                        prepared.len() as i64,
                        prepared.iter().map(|i| i.stops.len()).sum::<usize>() as i64,
                        prepared.iter().map(|i| i.tasks.len()).sum::<usize>() as i64,
                        ADAPTER_VERSION,
                        publication
                    ],
                )?;
                Ok(())
            })?;
            let fresh = db
                .one(
                    "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
                     FROM route_publications WHERE id=?",
                    [&publication],
                )?
                .ok_or_else(|| Error::new("not_found", 404))?;
            rebuilt.push(self::publication(&fresh));
        }
        Ok(RouteReprocess {
            publications: rebuilt,
        })
    }
    /// Every day with a publication at the DSP's station, newest first.
    fn route_days(&self, id: &str) -> Result<RouteDays> {
        let today = routes_today(self, id)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let days = db
            .all(
                "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
                 FROM route_publications WHERE station=? AND active=1 ORDER BY day DESC",
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
    fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>> {
        crate::validate::date(day)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let Some(row) = db.one(
            "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
             FROM route_publications WHERE station=? AND day=? AND active=1",
            [&station, day],
        )?
        else {
            return Ok(None);
        };
        let itineraries = db
            .all(
                &format!("{ITINERARY_SELECT} WHERE i.publication_id=? ORDER BY i.route_code,i.driver_name"),
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
    fn route_itinerary(
        &self,
        id: &str,
        day: &str,
        itinerary_id: &str,
    ) -> Result<Option<RouteItineraryDetail>> {
        crate::validate::date(day)?;
        let station = self.profile(id)?.station_code;
        let db = self.routes_db(id)?;
        let Some(row) = db.one(
            "SELECT id,day,mode,station,collected_at,route_count,itinerary_count,stop_count,task_count \
             FROM route_publications WHERE station=? AND day=? AND active=1",
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
        let tasks = db.all(
            "SELECT t.*,a.address1,a.city,a.state,a.postal_code,a.latitude address_latitude,a.longitude address_longitude \
             FROM tasks t LEFT JOIN addresses a ON a.address_id=t.address_id \
             WHERE t.publication_id=? AND t.itinerary_id=? ORDER BY t.executed_at,t.task_id",
            [&publication_id, itinerary_id],
        )?;
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
                tasks: tasks
                    .iter()
                    .filter(|t| t["stop_id"] == stop["stop_id"] && t["active"] != 0)
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
    fn route_package(&self, id: &str, tracking: &str) -> Result<RoutePackage> {
        let db = self.routes_db(id)?;
        let events = db
            .all(
                "SELECT t.*,i.driver_name,i.route_code,a.address1,a.city,a.state,a.postal_code,\
                 a.latitude address_latitude,a.longitude address_longitude \
                 FROM tasks t JOIN route_publications p ON p.id=t.publication_id AND p.active=1 \
                 LEFT JOIN itineraries i ON i.publication_id=t.publication_id AND i.itinerary_id=t.itinerary_id \
                 LEFT JOIN addresses a ON a.address_id=t.address_id \
                 WHERE t.tracking_id=? ORDER BY t.day DESC,t.executed_at,t.task_id LIMIT 500",
                [tracking],
            )?
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
}
/// Today, where the DSP is.
fn routes_today(store: &Store, id: &str) -> Result<NaiveDate> {
    let tz: chrono_tz::Tz = store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(chrono::Utc::now().with_timezone(&tz).date_naive())
}
/// The request for one day: the DSP's station and names, which the driver resolves
/// to a service area and provider on Cortex.
pub(crate) fn routes_request(store: &Store, id: &str, day: &str, mode: Mode) -> Result<Value> {
    let profile = store.profile(id)?;
    let dsp = store.find_dsp(id)?;
    ensure(
        !profile.station_code.is_empty(),
        "routes_station_required",
        409,
    )?;
    // A scope proven by an earlier day at this station spares discovery, which
    // starts from an address Cortex may send elsewhere.
    let known = store.routes_db(id)?.one(
        "SELECT service_area_id,provider FROM route_publications WHERE station=? AND active=1 \
         ORDER BY collected_at DESC LIMIT 1",
        [&profile.station_code],
    )?;
    let request = Request {
        collection: Collection::Routes,
        mode,
        date: day.into(),
        station: profile.station_code,
        timezone: dsp.timezone,
        dsp_name: dsp.name,
        dsp_abbreviation: profile.abbreviation,
        service_area_id: known.as_ref().map(|k| s(k, "service_area_id").to_owned()),
        provider: known.as_ref().map(|k| s(k, "provider").to_owned()),
    };
    request.validate()?;
    Ok(serde_json::to_value(request)?)
}
/// The first day the DSP's window keeps, or none when every day is kept.
fn retention_start(store: &Store, id: &str) -> Result<Option<NaiveDate>> {
    let days = store
        .routes_db(id)?
        .one("SELECT days FROM route_retention WHERE id=1", [])?
        .and_then(|row| row["days"].as_i64());
    Ok(match days {
        Some(days) => Some(routes_today(store, id)? - Duration::days(days)),
        None => None,
    })
}
/// Reuse the chosen publication during a scheduler batch. Every step checks its
/// activity and job again, because another transition can run between deletions.
pub(crate) fn sweep_routes_step(
    store: &Store,
    id: &str,
    selected: &mut Option<String>,
) -> Result<bool> {
    const BATCH: i64 = 2000;
    let db = store.routes_db(id)?;
    let running = store.active_job_ids(id, JOB_KIND)?;
    let retained = match selected.as_deref() {
        Some(publication) => db
            .one_as::<(String, String)>(
                "SELECT id,job_id FROM route_publications WHERE id=? AND active=0",
                [publication],
            )?
            .filter(|(_, job)| !running.contains(job))
            .map(|(publication, _)| publication),
        None => None,
    };
    let publication = match retained {
        Some(publication) => Some(publication),
        None => db
            .one_as::<(String,)>(
                "SELECT id FROM route_publications WHERE active=0 AND \
             job_id NOT IN (SELECT value FROM json_each(?)) ORDER BY collected_at,id LIMIT 1",
                [serde_json::to_string(&running)?],
            )?
            .map(|(publication,)| publication),
    };
    let Some(publication) = publication else {
        *selected = None;
        return Ok(false);
    };
    *selected = Some(publication.clone());
    let remaining = db.transaction(|| {
        for table in [
            "tasks",
            "stops",
            "route_raw",
            "breaks",
            "unknown_stops",
            "driver_days",
            "itineraries",
            "routes",
        ] {
            let removed = db.exec(
                &format!(
                    "DELETE FROM {table} WHERE rowid IN \
                     (SELECT rowid FROM {table} WHERE publication_id=? LIMIT ?)"
                ),
                params![publication, BATCH],
            )?;
            if removed > 0 {
                return Ok(true);
            }
        }
        db.exec("DELETE FROM route_publications WHERE id=?", [&publication])?;
        Ok(false)
    })?;
    if !remaining {
        *selected = None;
    }
    Ok(true)
}
/// A publication's capture as it was collected, from its stored responses.
fn stored_capture(db: &Db, row: &Value) -> Result<Capture> {
    let publication = s(row, "id");
    let declared: i64 = db.0.query_row(
        "SELECT COALESCE(SUM(raw_bytes),0) FROM route_raw WHERE publication_id=?",
        [publication],
        |row| row.get(0),
    )?;
    ensure(
        (0..=MAX_CAPTURE_BYTES as i64).contains(&declared),
        "routes_source_too_large",
        502,
    )?;
    let mut bytes = 0;
    let mut blob = |name: &str| -> Result<String> {
        let body: Vec<u8> = db.0.query_row(
            "SELECT body FROM route_raw WHERE publication_id=? AND name=?",
            [publication, name],
            |r| r.get(0),
        )?;
        let text = String::from_utf8(gunzip(&body)?)
            .map_err(|_| Error::new("routes_capture_invalid", 502))?;
        bytes = add_capture_bytes(bytes, text.len())?;
        Ok(text)
    };
    let scope = Scope {
        date: s(row, "day").into(),
        station: s(row, "station").into(),
        service_area_id: s(row, "service_area_id").into(),
        provider: s(row, "provider").into(),
        timezone: s(row, "timezone").into(),
    };
    let summaries: Value = serde_json::from_str(&blob("summaries")?)?;
    let route_summaries: Value = serde_json::from_str(&blob("route_summaries")?)?;
    let itineraries = listed(&summaries, &scope)
        .into_iter()
        .map(|(id, transporter_id)| {
            Ok(ItineraryCapture {
                detail: blob(&format!("itinerary:{id}"))?,
                id,
                transporter_id,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let ms = |key: &str| {
        chrono::DateTime::parse_from_rfc3339(s(row, key))
            .map(|t| t.timestamp_millis())
            .unwrap_or(0)
    };
    Ok(Capture {
        version: 1,
        collection: Collection::Routes,
        mode: Mode::parse(s(row, "mode")).unwrap_or_default(),
        scope,
        started_at: ms("started_at"),
        finished_at: ms("collected_at"),
        summaries,
        route_summaries,
        itineraries,
    })
}
/// The itinerary and day-summary columns a `RouteItinerary` is read from.
const ITINERARY_SELECT: &str = "SELECT i.itinerary_id,i.transporter_id,i.driver_name,i.route_code,i.execution_status,i.progress_status,\
    d.departed_at,d.first_stop_at,d.last_stop_at,d.session_end_at,d.stops_total,d.stops_completed,d.tasks_total,\
    d.delivered,d.picked_up,d.not_delivered,d.breaks_secs,d.overtime_secs \
    FROM itineraries i JOIN driver_days d ON d.publication_id=i.publication_id AND d.itinerary_id=i.itinerary_id";
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
/// The day's planned routes, inside the caller's transaction.
fn insert_routes(db: &Db, publication: &str, day: &str, routes: &[Value]) -> Result<()> {
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
fn insert_raw(db: &Db, publication: &str, name: &str, raw_bytes: usize, body: &[u8]) -> Result<()> {
    db.0.prepare_cached(
        "INSERT INTO route_raw(publication_id,name,encoding,raw_bytes,body) VALUES (?,?,'gzip',?,?)",
    )?
    .execute(params![publication, name, raw_bytes as i64, body])?;
    Ok(())
}
/// One shaped itinerary's rows under `publication`, inside the caller's transaction.
/// Its compressed response is stored beside them by staging, and kept by a reprocess.
fn insert_itinerary(
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
fn publication(row: &Value) -> RoutePublication {
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
fn sql_value(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;
    match value {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(text) => Sql::Text(text.clone()),
        other => Sql::Text(other.to_string()),
    }
}
fn opt<T: Into<Value>>(value: Option<T>) -> Value {
    value.map(Into::into).unwrap_or(Value::Null)
}
/// A list as JSON text, or nothing when absent or empty.
fn json_list(value: &Value) -> Value {
    match value.as_array() {
        Some(items) if !items.is_empty() => json!(Value::Array(items.clone()).to_string()),
        _ => Value::Null,
    }
}
/// The itinerary row's columns from the list's summary and the detail.
fn flatten(summary: &Value, detail: &Value, transporter: &str, driver_name: &str) -> Flat {
    let either = |key: &str| {
        if summary[key].is_null() {
            &detail[key]
        } else {
            &summary[key]
        }
    };
    let packages = &summary["packageSummary"];
    Flat {
        itinerary: vec![
            ("transporter_id", json!(transporter)),
            ("driver_name", json!(driver_name)),
            ("route_code", opt(text(either("routeCode")))),
            (
                "route_id",
                opt(text(&summary["routes"][0]["routeId"]).or_else(|| text(&detail["rmsRouteId"]))),
            ),
            ("company_id", opt(text(either("companyId")))),
            ("vin", opt(text(&summary["vinNumber"]))),
            ("service_type", opt(text(either("serviceTypeName")))),
            ("execution_status", opt(text(either("executionStatus")))),
            ("progress_status", opt(text(either("progressStatus")))),
            (
                "planned_departure_at",
                opt(stamp(either("plannedDepartureTime"))),
            ),
            (
                "departed_at",
                opt(stamp(
                    &detail["transporterTimeAttributes"]["actualDepartureTime"],
                )),
            ),
            ("started_at", opt(stamp(either("itineraryStartTime")))),
            ("wave_start_at", opt(stamp(either("waveStartTime")))),
            ("session_end_at", opt(stamp(either("sessionEndTime")))),
            (
                "projected_completion_at",
                opt(stamp(either("projectedCompletionTime"))),
            ),
            (
                "last_stop_at",
                opt(stamp(&summary["lastStopExecutionTime"])),
            ),
            (
                "block_minutes",
                opt(integer(either("blockDurationInMinutes"))),
            ),
            ("total_locations", opt(integer(&summary["totalLocations"]))),
            (
                "completed_locations",
                opt(integer(&summary["completedLocations"])),
            ),
            ("total_packages", opt(integer(&summary["totalPackages"]))),
            ("delivered", opt(integer(&packages["DELIVERED"]))),
            ("remaining", opt(integer(&packages["REMAINING"]))),
            ("undeliverable", opt(integer(&packages["UNDELIVERABLE"]))),
            ("stops_impacted", opt(integer(either("stopsImpacted")))),
            (
                "packages_impacted",
                opt(integer(either("packagesImpacted"))),
            ),
            (
                "breaks_secs",
                opt(integer(&summary["totalBreaksDurationSecs"])),
            ),
            (
                "overtime_secs",
                opt(integer(either("overtimeDurationSecs"))),
            ),
            (
                "stop_completion_rate",
                opt(number(either("stopCompletionRate"))),
            ),
            (
                "breaks",
                json!(
                    either("breaks")
                        .as_array()
                        .map(|v| Value::Array(v.clone()).to_string())
                        .unwrap_or_else(|| "[]".into())
                ),
            ),
            (
                "planned_breaks",
                json!(
                    either("plannedBreaks")
                        .as_array()
                        .map(|v| Value::Array(v.clone()).to_string())
                        .unwrap_or_else(|| "[]".into())
                ),
            ),
            ("rescue_actions", json_list(either("rescueActions"))),
            ("sequence_edits", json_list(either("sequenceEdits"))),
            ("pause_events", json_list(either("routePauseEvents"))),
            (
                "summary",
                json!(if summary.is_null() {
                    "{}".to_owned()
                } else {
                    summary.to_string()
                }),
            ),
        ],
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct StopRow {
    id: String,
    sequence: Option<i64>,
    address_id: Option<String>,
    stop_type: Option<String>,
    route_code: Option<String>,
    planned_start_at: Option<i64>,
    planned_end_at: Option<i64>,
    expected_start_at: Option<i64>,
    actual_start_at: Option<i64>,
    flags: Value,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct TaskRow {
    id: String,
    stop_id: String,
    tracking_id: Option<String>,
    order_id: Option<String>,
    task_type: Option<String>,
    task_state: Option<String>,
    state_context: Option<String>,
    execution_status: Option<String>,
    address_id: Option<String>,
    window_start_at: Option<i64>,
    window_end_at: Option<i64>,
    executed_at: Option<i64>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    box_type: Option<String>,
    weight: Option<f64>,
    weight_unit: Option<String>,
    length: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    volume_unit: Option<String>,
    time_windowed: Option<i64>,
    high_value: Option<i64>,
    customer_return: Option<i64>,
    address_type: Option<String>,
    events: Value,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct DayRow {
    route_code: Option<String>,
    departed_at: Option<i64>,
    session_end_at: Option<i64>,
    breaks_secs: Option<i64>,
    overtime_secs: Option<i64>,
    first_stop_at: Option<i64>,
    last_stop_at: Option<i64>,
    stops_total: i64,
    stops_completed: i64,
    tasks_total: i64,
    delivered: i64,
    picked_up: i64,
    not_delivered: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct BreakRow {
    planned: bool,
    break_id: Option<String>,
    kind: Option<String>,
    state: Option<String>,
    sequence: Option<i64>,
    punch_id: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    planned_start_at: Option<i64>,
    planned_end_at: Option<i64>,
    min_duration_ms: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct UnknownStopRow {
    entered_at: Option<i64>,
    exited_at: Option<i64>,
    enter_latitude: Option<f64>,
    enter_longitude: Option<f64>,
    exit_latitude: Option<f64>,
    exit_longitude: Option<f64>,
}
/// A task's row, at `stop_id` or removed from the route. A task without an identifier
/// cannot be a row.
fn task_row(task: &Value, stop_id: &str) -> Option<TaskRow> {
    let task_id = text(&task["taskId"]).filter(|v| identifier(v, 256))?;
    let dimension = |key: &str| number(&task[key]);
    Some(TaskRow {
        id: task_id,
        stop_id: stop_id.to_owned(),
        tracking_id: text(&task["domainMap"]["scannableId"]).or_else(|| text(&task["scannableId"])),
        order_id: text(&task["domainMap"]["orderId"]),
        task_type: text(&task["taskType"]),
        task_state: text(&task["taskState"]),
        state_context: text(&task["taskStateContext"]),
        execution_status: text(&task["executionStatus"]),
        address_id: text(&task["addressId"]),
        window_start_at: stamp(&task["windowStartTime"]),
        window_end_at: stamp(&task["windowEndTime"]),
        executed_at: stamp(&task["actualExecutionTime"])
            .or_else(|| stamp(&task["taskExecutionTime"])),
        latitude: number(&task["executionGeocode"]["latitude"]),
        longitude: number(&task["executionGeocode"]["longitude"]),
        box_type: text(&task["boxType"]),
        weight: dimension("weight"),
        weight_unit: text(&task["weightUnit"]),
        length: dimension("length"),
        width: dimension("width"),
        height: dimension("height"),
        volume_unit: text(&task["volumeUnit"]),
        time_windowed: flag(&task["timeWindowed"]),
        high_value: flag(&task["highValueTaskFlag"]),
        customer_return: flag(&task["customerReturn"]),
        address_type: text(&task["addressTypeInfo"]),
        events: compact_events(&task["recentTaskEvents"]),
    })
}
/// A task's recent events as [time, state, context] triples: every event Amazon sends
/// carries just these three, and their names repeated in every row would be a third of
/// the table.
fn compact_events(events: &Value) -> Value {
    Value::Array(
        events
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| json!([e["executionTime"], e["taskState"], e["taskStateContext"]]))
            .collect(),
    )
}
/// The stops and tasks of a detail, and the day summary they add up to. A task or
/// stop without an identifier, or one repeated, is skipped: it cannot be a row.
fn rows(detail: &Value, transporter: &str) -> (Vec<StopRow>, Vec<TaskRow>, DayRow) {
    let mut stops = Vec::new();
    let mut tasks = Vec::new();
    let mut day = DayRow::default();
    let mut seen_stops = std::collections::HashSet::new();
    let mut seen_tasks = std::collections::HashSet::new();
    for (index, stop) in detail["stops"].as_array().into_iter().flatten().enumerate() {
        // The API sends no stop id; the page adds one. The sequence number is the
        // stop's identity within its itinerary, and its position stands in without one.
        let stop_id = text(&stop["stopId"])
            .filter(|v| identifier(v, 256))
            .or_else(|| integer(&stop["sequenceNumber"]).map(|n| format!("seq-{n}")))
            .unwrap_or_else(|| format!("index-{index}"));
        if !seen_stops.insert(stop_id.clone()) {
            continue;
        }
        let mut completed = false;
        for task in stop["tasks"].as_array().into_iter().flatten() {
            // A task another driver executed belongs to that driver's itinerary.
            if task["transporterId"]
                .as_str()
                .is_some_and(|t| t != transporter)
            {
                continue;
            }
            let Some(row) = task_row(task, &stop_id) else {
                continue;
            };
            if !seen_tasks.insert(row.id.clone()) {
                continue;
            }
            day.tasks_total += 1;
            match (row.task_type.as_deref(), row.task_state.as_deref()) {
                (Some("DROP_OFF"), Some("DELIVERED")) => {
                    day.delivered += 1;
                    completed = true;
                    if let Some(at) = row.executed_at {
                        day.first_stop_at = Some(day.first_stop_at.map_or(at, |v| v.min(at)));
                        day.last_stop_at = Some(day.last_stop_at.map_or(at, |v| v.max(at)));
                    }
                }
                (Some("DROP_OFF"), _) => day.not_delivered += 1,
                (Some("PICK_UP"), Some("PICKED_UP")) => day.picked_up += 1,
                _ => (),
            }
            tasks.push(row);
        }
        day.stops_total += 1;
        if completed {
            day.stops_completed += 1;
        }
        let mut flags = stop.clone();
        if let Some(object) = flags.as_object_mut() {
            object.retain(|k, v| k.ends_with("Flag") || (k.starts_with("is") && v.is_boolean()));
        }
        stops.push(StopRow {
            id: stop_id,
            sequence: integer(&stop["sequenceNumber"]),
            address_id: text(&stop["addressId"]),
            stop_type: text(&stop["itineraryStopType"]),
            route_code: text(&stop["routeCode"]),
            planned_start_at: stamp(&stop["plannedStartTime"]),
            planned_end_at: stamp(&stop["plannedEndTime"]),
            expected_start_at: stamp(&stop["expectedStartTime"]),
            actual_start_at: stamp(&stop["actualStartTime"]),
            flags: set_flags(&flags),
        });
    }
    if let Some(completed) = integer(&detail["stopProgress"]["completed"]) {
        day.stops_completed = completed;
    }
    (stops, tasks, day)
}
/// Tasks removed from the route, each once.
fn inactive_tasks(detail: &Value) -> Vec<TaskRow> {
    let mut seen = std::collections::HashSet::new();
    detail["inactiveTasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|task| task_row(task, ""))
        .filter(|row| seen.insert(row.id.clone()))
        .collect()
}
/// Break punches, from the detail or else the list's summary, then planned breaks.
fn break_rows(summary: &Value, detail: &Value) -> Vec<BreakRow> {
    let taken = match detail["breaks"].as_array() {
        Some(list) if !list.is_empty() => Some(list),
        _ => summary["breaks"].as_array(),
    };
    let planned = match detail["plannedBreaks"].as_array() {
        Some(list) if !list.is_empty() => Some(list),
        _ => summary["plannedBreaks"].as_array(),
    };
    let mut rows: Vec<BreakRow> = taken
        .into_iter()
        .flatten()
        .map(|b| BreakRow {
            planned: false,
            break_id: text(&b["breakId"]),
            kind: text(&b["type"]),
            state: text(&b["state"]),
            sequence: integer(&b["sequenceNumber"]),
            punch_id: text(&b["punchId"]),
            started_at: stamp(&b["timeStampOn"]),
            ended_at: stamp(&b["timeStampOff"]),
            planned_start_at: None,
            planned_end_at: None,
            min_duration_ms: None,
        })
        .collect();
    rows.extend(planned.into_iter().flatten().map(|b| BreakRow {
        planned: true,
        break_id: text(&b["breakId"]),
        kind: text(&b["type"]).or_else(|| text(&b["breakType"])),
        state: None,
        sequence: integer(&b["plannedSequenceNumber"]),
        punch_id: None,
        started_at: None,
        ended_at: None,
        planned_start_at: stamp(&b["plannedStart"]),
        planned_end_at: stamp(&b["plannedEnd"]),
        min_duration_ms: integer(&b["minDurationInMillis"]),
    }));
    rows
}
fn unknown_stop_rows(detail: &Value) -> Vec<UnknownStopRow> {
    detail["unknownStops"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|u| UnknownStopRow {
            entered_at: stamp(&u["enterTime"]),
            exited_at: stamp(&u["exitTime"]),
            enter_latitude: number(&u["enterLatitude"]),
            enter_longitude: number(&u["enterLongitude"]),
            exit_latitude: number(&u["exitLatitude"]),
            exit_longitude: number(&u["exitLongitude"]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collectors::cortex::{
        discovery::CollectionRequest,
        routes::{MAX_ITINERARIES, fixture},
    };
    #[test]
    fn retained_sweep_selection_rechecks_running_jobs_and_publication_activity() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        crate::db::private_dir(root.path()).unwrap();
        let mut config = crate::config::Config::load().unwrap();
        config.root = root.path().into();
        config.fixture = true;
        config.development = true;
        config.environment = "preview".into();
        let db = Store::initialize(config).unwrap();
        let bootstrap = crate::operations::bootstrap(
            &db,
            "sweep@example.test",
            "Sweep",
            "Owner",
            "sweep-password-test",
        )
        .unwrap();
        let dsp = s(&bootstrap["dsp"], "id");
        db.set_profile(
            dsp,
            json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
        )
        .unwrap();
        db.collector(dsp, crate::collectors::cortex::PROVIDER)
            .unwrap()
            .exec("UPDATE connections SET enabled=1,status='ready'", [])
            .unwrap();
        let queued = db
            .enqueue_routes(
                dsp,
                None,
                "sweep-selection",
                Some("2026-09-25"),
                Mode::Final,
                1,
            )
            .unwrap();
        let job = s(&queued[0], "id");
        let staged = db
            .stage_routes(dsp, job, fixture(&request(Mode::Final)).unwrap())
            .unwrap();
        db.jobs
            .exec("UPDATE jobs SET status='failed' WHERE id=?", [job])
            .unwrap();
        let mut selected = None;
        assert!(sweep_routes_step(&db, dsp, &mut selected).unwrap());
        assert_eq!(selected.as_deref(), Some(staged.publication.as_str()));
        let remaining = || {
            db.routes_db(dsp)
                .unwrap()
                .count("SELECT count(*) FROM stops", [])
                .unwrap()
        };
        let stops = remaining();
        assert!(stops > 0);
        // An intervening job transition must protect even the retained candidate.
        db.jobs
            .exec("UPDATE jobs SET status='queued' WHERE id=?", [job])
            .unwrap();
        assert!(!sweep_routes_step(&db, dsp, &mut selected).unwrap());
        assert_eq!(remaining(), stops);
        assert!(selected.is_none());
        db.jobs
            .exec("UPDATE jobs SET status='failed' WHERE id=?", [job])
            .unwrap();
        selected = Some(staged.publication.clone());
        db.routes_db(dsp)
            .unwrap()
            .exec(
                "UPDATE route_publications SET active=1 WHERE id=?",
                [&staged.publication],
            )
            .unwrap();
        assert!(!sweep_routes_step(&db, dsp, &mut selected).unwrap());
        assert_eq!(remaining(), stops);
        assert!(selected.is_none());
        db.routes_db(dsp)
            .unwrap()
            .exec(
                "UPDATE route_publications SET active=0 WHERE id=?",
                [&staged.publication],
            )
            .unwrap();
        while sweep_routes_step(&db, dsp, &mut selected).unwrap() {}
        assert!(selected.is_none());
        assert_eq!(
            db.routes_db(dsp)
                .unwrap()
                .count("SELECT count(*) FROM route_publications", [])
                .unwrap(),
            0
        );
    }
    fn request(mode: Mode) -> Request {
        Request {
            collection: Collection::Routes,
            mode,
            date: "2026-09-25".into(),
            station: "TST1".into(),
            timezone: "America/Los_Angeles".into(),
            dsp_name: "Northline Logistics".into(),
            dsp_abbreviation: "NLOG".into(),
            service_area_id: None,
            provider: None,
        }
    }
    #[test]
    fn requests_are_recognized_and_carry_a_scope_once_resolved() {
        assert!(
            Request::parse(&json!({"date":"2026-09-25"}))
                .unwrap()
                .is_none()
        );
        let value = serde_json::to_value(request(Mode::Final)).unwrap();
        assert_eq!(value["collection"], "routes");
        assert_eq!(value["mode"], "final");
        assert!(value.get("serviceAreaId").is_none());
        let parsed = Request::parse(&value).unwrap().unwrap();
        assert!(matches!(
            parsed.scope_request(),
            CollectionRequest::Discover(_)
        ));
        let mut scoped = request(Mode::Snapshot);
        scoped.service_area_id = Some("area-1".into());
        scoped.provider = Some("provider-1".into());
        assert!(matches!(
            scoped.scope_request(),
            CollectionRequest::Scoped(_)
        ));
        let mut half = request(Mode::Final);
        half.provider = Some("provider-1".into());
        assert!(half.validate().is_err());
        assert!(
            Request::parse(&json!({"collection":"routes","date":"2026-09-25","station":"tst1"}))
                .is_err()
        );
    }
    #[test]
    fn days_follow_the_mode() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 26).unwrap();
        assert_eq!(default_day(Mode::Final, today), "2026-09-25");
        assert_eq!(default_day(Mode::Snapshot, today), "2026-09-26");
        assert!(day_allowed(Mode::Final, "2026-09-25", today).is_ok());
        assert!(day_allowed(Mode::Final, "2026-09-26", today).is_err());
        assert!(day_allowed(Mode::Snapshot, "2026-09-26", today).is_ok());
        assert!(day_allowed(Mode::Snapshot, "2026-09-25", today).is_err());
        assert!(day_allowed(Mode::Final, "2026-9-25", today).is_err());
    }
    #[test]
    fn the_fixture_is_a_valid_day_whose_rows_add_up() {
        let capture = fixture(&request(Mode::Final)).unwrap();
        assert_eq!(capture.itineraries.len(), 2);
        let (stops, tasks, day) = rows(
            &capture.itineraries[1].parsed().unwrap()["itineraryDetails"],
            "driver-2",
        );
        assert_eq!((stops.len(), tasks.len()), (2, 4));
        assert_eq!((day.delivered, day.picked_up, day.not_delivered), (1, 2, 1));
        assert_eq!((day.stops_total, day.stops_completed), (2, 2));
        assert_eq!(tasks[0].tracking_id.as_deref(), Some("TBA000000000001"));
        assert_eq!(stops[0].flags, json!(["dispatchStopFlag"]));
        assert!(day.first_stop_at.unwrap() <= day.last_stop_at.unwrap());
        let inflated = gunzip(&gzip(b"{\"a\":1}").unwrap()).unwrap();
        assert_eq!(inflated, b"{\"a\":1}");
    }
    #[test]
    fn aggregate_response_bytes_stop_at_the_daily_limit() {
        let chunk = MAX_CAPTURE_BYTES / MAX_ITINERARIES;
        let mut total = 0;
        for _ in 0..MAX_ITINERARIES {
            total = add_capture_bytes(total, chunk).unwrap();
        }
        total = add_capture_bytes(total, MAX_CAPTURE_BYTES - total).unwrap();
        assert_eq!(total, MAX_CAPTURE_BYTES);
        let error = add_capture_bytes(total, 1).unwrap_err();
        assert_eq!(error.code, "routes_source_too_large");
    }
    #[test]
    fn stamps_take_seconds_and_milliseconds() {
        assert_eq!(stamp(&json!(1_758_800_000)), Some(1_758_800_000_000));
        assert_eq!(stamp(&json!(1_758_800_000_123i64)), Some(1_758_800_000_123));
        assert_eq!(stamp(&json!(null)), None);
        assert_eq!(stamp(&json!(-5)), None);
    }
}
