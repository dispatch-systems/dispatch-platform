//! Daily routes from Cortex's execution pages: the station's route list, every driver's
//! itinerary with its stops and packages, and the newer routes page's list, one day at a
//! time, published into the DSP's route data database. Normalized rows hold what reads
//! filter on; every response is kept whole, compressed, for reprocessing. Browser and
//! HTTP data is untrusted input. The collection is `routes` to the platform; its database is
//! `routedata`.
pub(crate) mod exposed;
pub(crate) mod keeper;
pub(crate) mod maintenance;
pub(crate) mod people;
mod publishing;
mod rows;
mod storage;

use publishing::*;
use rows::*;
pub use storage::RoutesStore;
use storage::*;

use crate::api::types::{
    RouteAddress, RouteBreak, RouteDayView, RouteDays, RouteItinerary, RouteItineraryDetail,
    RoutePackage, RoutePackageEvent, RoutePublication, RouteReprocess, RouteRetention, RouteStop,
    RouteTask, RouteUnknownStop,
};
use chrono::{Duration, NaiveDate};
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, at, migrations::add_column, s},
    ensure,
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
/// The names of an object's flags that are set, for a compact record of a stop's kinds.
pub(super) fn set_flags(value: &Value) -> Value {
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
    state: &std::sync::Arc<dispatch_core::State>,
    dsp: &str,
    job: &str,
    owner: &str,
    mut capture: Capture,
) -> Result<Staged> {
    let blocking = |error: tokio::task::JoinError| {
        dispatch_core::foundation::observability::event(
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

/// Today, where the DSP is.
pub(super) fn routes_today(store: &Store, id: &str) -> Result<NaiveDate> {
    let tz: chrono_tz::Tz = store
        .find_dsp(id)?
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    Ok(dispatch_core::foundation::clock::today(tz))
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
pub(super) fn retention_start(store: &Store, id: &str) -> Result<Option<NaiveDate>> {
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
