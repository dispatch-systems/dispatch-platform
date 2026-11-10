//! Routes storage registration, migrations and capture restoration, with one DSP store facade.
mod commands;
mod reads;
mod reprocess;

pub(super) use commands::stage_prepared_routes_start;

use super::{
    maintenance,
    shaping::{PreparedItinerary, gunzip},
    staging::Staged,
};
use crate::api::types::{
    RouteDayView, RouteDays, RouteItineraryDetail, RoutePackage, RouteReprocess,
};
use dispatch_core::{
    Error, Result,
    collection::registry::AddedStorage,
    db::{Db, DspLease, Kind, Store, migrations::add_column, s},
    ensure,
};
use dispatch_cortex::{
    self as cortex,
    discovery::Scope,
    routes::{
        Capture, Collection, ItineraryCapture, MAX_CAPTURE_BYTES, Mode, add_capture_bytes, listed,
    },
};
use serde_json::{Value, json};

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
/// Migration 10: each itinerary keeps its work phone, found by the phone too.
pub(crate) fn add_work_phone(db: &Db) -> Result<()> {
    add_column(db, "itineraries", "work_phone", "TEXT")?;
    db.0.execute_batch(include_str!(
        "../migrations/routedata/0010_itinerary_work_phone.sql"
    ))?;
    Ok(())
}
/// Live route data's migration 2: each itinerary keeps its work phone, as in historical route
/// data; a day's one read needs no index to find it by.
pub(crate) fn add_live_work_phone(db: &Db) -> Result<()> {
    add_column(db, "itineraries", "work_phone", "TEXT")
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
    for table in READ_TABLES {
        db.one(&format!("SELECT count(*) FROM {table} WHERE 0"), [])?;
    }
    Ok(())
}

/// What Routes reads and writes for a DSP: each day's routes, itineraries and packages, in
/// the database beside Cortex's, the collections that bring them and their expiry.
pub trait RoutesStore {
    /// Historical route data: the days kept.
    fn routes_db(&self, id: &str) -> Result<DspLease<'_>>;
    /// Live route data: today as the last whole live read left it.
    fn routes_live_db(&self, id: &str) -> Result<DspLease<'_>>;
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
    fn routes_jobs(&self, id: &str, mode: Mode) -> Result<Vec<(String, Value)>>;
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()>;
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<bool>;
    fn expire_routes(&self, id: &str) -> Result<usize>;
    /// One small step of removing what no reader sees: publications a newer one
    /// replaced, a retention window retired, or a job left unfinished. Answers whether
    /// more remains. A publication whose job still runs is being staged and is kept.
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
    fn routes_live_db(&self, id: &str) -> Result<DspLease<'_>> {
        self.added_storage(id, cortex::PROVIDER, &LIVE_STORAGE)
    }
    fn enqueue_routes(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        day: Option<&str>,
        mode: Mode,
        days: i64,
    ) -> Result<Value> {
        commands::enqueue_routes(self, id, actor, key, day, mode, days)
    }
    fn routes_schedule_ready(&self, id: &str) -> Result<()> {
        commands::routes_schedule_ready(self, id)
    }
    fn routes_jobs(&self, id: &str, mode: Mode) -> Result<Vec<(String, Value)>> {
        commands::routes_jobs(self, id, mode)
    }
    fn stage_routes_itinerary(
        &self,
        id: &str,
        staged: &Staged,
        itinerary: &PreparedItinerary,
    ) -> Result<()> {
        commands::stage_routes_itinerary(self, id, staged, itinerary)
    }
    fn publish_routes(&self, id: &str, job: &str, staged: &Staged) -> Result<bool> {
        commands::publish_routes(self, id, job, staged)
    }
    fn expire_routes(&self, id: &str) -> Result<usize> {
        maintenance::expire_routes(self, id)
    }
    fn sweep_routes(&self, id: &str) -> Result<bool> {
        maintenance::sweep_routes_step(self, id, &mut None)
    }
    fn reprocess_routes(&self, id: &str, day: Option<&str>) -> Result<RouteReprocess> {
        reprocess::reprocess_routes(self, id, day)
    }
    fn route_days(&self, id: &str) -> Result<RouteDays> {
        reads::route_days(self, id)
    }
    fn route_day(&self, id: &str, day: &str) -> Result<Option<RouteDayView>> {
        reads::route_day(self, id, day)
    }
    fn route_itinerary(
        &self,
        id: &str,
        day: &str,
        itinerary_id: &str,
    ) -> Result<Option<RouteItineraryDetail>> {
        reads::route_itinerary(self, id, day, itinerary_id)
    }
    fn route_package(&self, id: &str, tracking: &str) -> Result<RoutePackage> {
        reads::route_package(self, id, tracking)
    }
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
