//! Daily routes from Cortex's execution pages: the station's route list, every driver's
//! itinerary with its stops and packages, and the newer routes page's list, one day at a
//! time, published into the DSP's route data database. Normalized rows hold what reads
//! filter on; every response is kept whole, compressed, for reprocessing. Browser and
//! HTTP data is untrusted input. The collection is `routes` to the platform; its database is
//! `routedata`, historical route data. Today as live reads leave it is live route data, in a
//! database of its own, `routelive`, which a feature reading routes live keeps (`live`).
pub(crate) mod exposed;
pub(crate) mod keeper;
mod live;
pub(crate) mod maintenance;
pub(crate) mod people;
mod phones;
mod publishing;
mod rows;
mod saving;
pub(crate) mod settings;
mod storage;
mod wire;

pub use live::{LiveKept, clear_live, keep_live, save_live_day};
use publishing::*;
use rows::*;
pub use saving::{WorkPhone, save_day};
pub use storage::RoutesStore;
use storage::*;

use crate::api::types::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteStop, RouteTask,
    RouteUnknownStop,
};
use chrono::{Duration, NaiveDate};
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, at, migrations::add_column, s},
    ensure,
    foundation::observability,
    server::cache::DataDomain,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    routes::{
        Capture, Collection, ItineraryCapture, JOB_KIND, MAX_BODY, MAX_CAPTURE_BYTES, Mode,
        Request, add_capture_bytes, listed,
    },
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::Write;
pub const COLLECTION: &str = "routes";
/// What a schedule collects by to read today so far, rather than yesterday's finished day
/// and the days before it lacks: an alias of the routes collection. Permanent: schedules
/// store it.
pub const TODAY: &str = "routes_today";
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
/// Live route data, beside historical route data: today alone, in the tables historical route
/// data keeps its days in.
pub const LIVE_DATABASE: Kind = Kind::new("routelive", 1).cache(8192);
pub static LIVE_STORAGE: AddedStorage = AddedStorage {
    id: "routelive",
    kind: LIVE_DATABASE,
    marker: "storage.routelive",
    source: "routelive-v1",
    verify: verify_live,
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
    /// The work phones its response shows its drivers with, by their ten digits.
    work_phones: Vec<(String, String)>,
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
pub(super) fn text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.chars().take(512).collect()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
pub(super) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
pub(super) fn integer(value: &Value) -> Option<i64> {
    number(value).map(|n| n.round() as i64)
}
pub(super) fn flag(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(flag) => Some(i64::from(*flag)),
        _ => None,
    }
}
pub(super) fn gzip(bytes: &[u8]) -> Result<Vec<u8>> {
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

/// What shaping takes from an itinerary's response, however it was read: its detail's
/// route-level fields, the rows of its stops and tasks, and the drivers and addresses it
/// names.
struct Read<'a> {
    detail: &'a Value,
    transporters: &'a Value,
    addresses: &'a Value,
    stops: Vec<StopRow>,
    tasks: Vec<TaskRow>,
    day: DayRow,
    inactive: Vec<TaskRow>,
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
    /// One itinerary's rows and its compressed response. Its stops and tasks, most of it, are
    /// read straight into what shaping reads of them (`wire`), without a tree; that reading
    /// lives only while this runs.
    pub fn shape(&self, captured: &ItineraryCapture) -> Result<PreparedItinerary> {
        ensure(
            captured.detail.len() <= MAX_BODY,
            "routes_source_too_large",
            502,
        )?;
        let invalid = || Error::new("routes_capture_invalid", 502);
        let response = wire::read(&captured.detail).map_err(|_| invalid())?;
        let read = response.detail.0.ok_or_else(invalid)?;
        let detail = Value::Object(read.rest);
        let (stops, tasks, day) = stop_rows(
            read.stops.0.iter(),
            &detail["stopProgress"],
            &captured.transporter_id,
        );
        let inactive = inactive_rows(read.inactive.0.iter());
        drop((read.stops, read.inactive));
        let read = Read {
            detail: &detail,
            transporters: &response.transporters,
            addresses: &response.addresses,
            stops,
            tasks,
            day,
            inactive,
        };
        self.shape_read(
            &captured.id,
            &captured.transporter_id,
            read,
            captured.detail.len(),
            gzip(captured.detail.as_bytes())?,
        )
    }
    /// One itinerary's rows from what was read of its response, with `raw`, the response
    /// compressed, kept beside them.
    fn shape_read(
        &self,
        id: &str,
        transporter_id: &str,
        read: Read,
        raw_bytes: usize,
        raw: Vec<u8>,
    ) -> Result<PreparedItinerary> {
        let detail = read.detail;
        let summary = self.summaries.get(id).cloned().unwrap_or(Value::Null);
        let person = self
            .people
            .iter()
            .chain(read.transporters.as_array().into_iter().flatten())
            .find(|t| t["transporterId"] == json!(transporter_id))
            .cloned()
            .unwrap_or(Value::Null);
        let driver_name = match (text(&person["firstName"]), text(&person["lastName"])) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            _ => text(&detail["transporterName"]).unwrap_or_else(|| transporter_id.to_owned()),
        };
        let flat = flatten(&summary, detail, transporter_id, &driver_name);
        Ok(PreparedItinerary {
            id: id.to_owned(),
            transporter_id: transporter_id.to_owned(),
            columns: flat
                .itinerary
                .into_iter()
                .map(|(c, v)| (c.to_owned(), v))
                .collect(),
            stops: read.stops,
            tasks: read.tasks,
            inactive: read.inactive,
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
                ..read.day
            },
            addresses: read.addresses.as_array().cloned().unwrap_or_default(),
            person,
            work_phones: (phones::phones_of(read.transporters))
                .filter_map(|(driver, phone)| Some((driver, phone?)))
                .collect(),
            raw_bytes,
            raw,
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

/// Stores a finished day as an inactive publication of its job, one itinerary at a time, each
/// insert first checking the job is still this worker's.
pub async fn stage(
    state: &std::sync::Arc<dispatch_core::State>,
    dsp: &str,
    job: &str,
    owner: &str,
    capture: Capture,
) -> Result<Staged> {
    let (d, j) = (dsp.to_owned(), job.to_owned());
    let request = state
        .read(move |db| Ok(db.job_row(&j, Some(&d))?.request))
        .await?;
    let request = Request::parse(&serde_json::from_str(&request)?)?
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let (j, o) = (job.to_owned(), owner.to_owned());
    stage_capture(state, dsp, job, request, capture, move |db| {
        db.guard(&j, &o).map(drop)
    })
    .await
}
/// Stores a day as an inactive publication under `key`, one itinerary at a time, once
/// `capture` is checked whole against `request`. Checking and shaping happen on blocking
/// threads without the platform lock; each insert takes it only for that itinerary's rows,
/// and first asks `mine` whether the work is still the caller's. Only one itinerary's
/// reading and rows exist at once.
async fn stage_capture(
    state: &std::sync::Arc<dispatch_core::State>,
    dsp: &str,
    key: &str,
    request: Request,
    mut capture: Capture,
    mine: impl Fn(&Store) -> Result<()> + Clone + Send + 'static,
) -> Result<Staged> {
    let blocking = blocking("routes.stage_failed");
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
    let (d, k, check) = (dsp.to_owned(), key.to_owned(), mine.clone());
    let staged = state
        .run_scoped(dsp, DOMAIN, move |db| {
            check(db)?;
            db.stage_routes_start(&d, &k, &capture, count)
        })
        .await?;
    for captured in itineraries {
        let shaper = shaper.clone();
        let shaped = tokio::task::spawn_blocking(move || shaper.shape(&captured))
            .await
            .map_err(blocking)??;
        let (d, staged, check) = (dsp.to_owned(), staged.clone(), mine.clone());
        state
            .run_scoped(dsp, DOMAIN, move |db| {
                check(db)?;
                db.stage_routes_itinerary(&d, &staged, &shaped)
            })
            .await?;
    }
    Ok(staged)
}

/// A blocking thread's end, logged as `event`, when it ends before its work does: only a panic
/// ends one.
fn blocking(event: &'static str) -> impl Fn(tokio::task::JoinError) -> Error + Copy {
    move |error| {
        observability::event("error", event, json!({"panic":error.is_panic()}));
        Error::new("routes_capture_invalid", 500)
    }
}

/// The tables route data reads: missing once initialized, they fail closed, rather than
/// recreating lost data.
const READ_TABLES: [&str; 9] = [
    "route_publications",
    "routes",
    "itineraries",
    "stops",
    "tasks",
    "addresses",
    "drivers",
    "driver_days",
    "route_raw",
];
fn verify(db: &Db) -> Result<()> {
    ensure(
        db.all("SELECT version FROM routedata_schema", [])? == vec![json!({"version":1})],
        "unsupported_routedata_schema",
        503,
    )?;
    for table in READ_TABLES {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}
fn verify_live(db: &Db) -> Result<()> {
    for table in READ_TABLES
        .iter()
        .chain(&["work_phones", "work_phones_shown"])
    {
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

/// Today, where the DSP is.
pub(super) fn routes_today(store: &Store, id: &str) -> Result<NaiveDate> {
    routes_day(store, id, dispatch_core::db::now())
}
/// The day it is where the DSP is at `at`, in milliseconds since the epoch.
fn routes_day(store: &Store, id: &str, at: i64) -> Result<NaiveDate> {
    let tz: chrono_tz::Tz = store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(dispatch_core::foundation::clock::day(tz, at))
}
/// The request for one day: the DSP's station and names, which the driver resolves
/// to a service area and provider on Cortex.
pub(crate) fn routes_request(store: &Store, id: &str, day: &str, mode: Mode) -> Result<Value> {
    Ok(serde_json::to_value(day_request(store, id, day, mode)?)?)
}
/// What a job reading `day` in `mode` asks Cortex for, which a capture of it must match.
fn day_request(store: &Store, id: &str, day: &str, mode: Mode) -> Result<Request> {
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
    Ok(request)
}
/// The first day the DSP's window keeps, or none when every day is kept.
pub(super) fn retention_start(store: &Store, id: &str) -> Result<Option<NaiveDate>> {
    let retention: settings::Retention =
        dispatch_core::settings::read(store, id, &settings::RETENTION)?;
    (retention.days)
        .map(|days| first_kept(store, id, days, dispatch_core::db::now()))
        .transpose()
}
/// The first day a window of `days` keeps at `at`, in milliseconds since the epoch: that many
/// days back from the day it is then where the DSP is, as expiry counts it as it runs.
pub(super) fn first_kept(store: &Store, id: &str, days: i64, at: i64) -> Result<NaiveDate> {
    Ok(routes_day(store, id, at)? - Duration::days(days))
}
/// Reuse the chosen publication during a scheduler batch. Every step checks its
/// activity and job again, because another transition can run between deletions. A
/// publication a job or a save is still staging is kept.
pub(crate) fn sweep_routes_step(
    store: &Store,
    id: &str,
    selected: &mut Option<String>,
) -> Result<bool> {
    const BATCH: i64 = 2000;
    let db = store.routes_db(id)?;
    let mut running = store.active_job_ids(id, JOB_KIND)?;
    running.extend(saving::running(id));
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
            "staged_addresses",
            "staged_drivers",
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
pub(super) fn stored_capture(db: &Db, row: &Value) -> Result<Capture> {
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

#[cfg(test)]
#[path = "../tests/backend/mod.rs"]
mod tests;
