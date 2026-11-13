//! Routes' facade and shared identities. Historical (`routedata`) and live (`routelive`)
//! databases keep normalized rows and whole compressed responses for reprocessing.
//! Browser and HTTP captures are untrusted input.
pub(crate) mod changed;
pub(crate) mod exposed;
pub(crate) mod keeper;
mod live;
pub(crate) mod maintenance;
pub(crate) mod people;
mod phone_backfill;
mod publishing;
mod rows;
mod saving;
mod scope;
pub(crate) mod settings;
mod shaping;
mod staging;
mod storage;
mod wire;
mod work_phones;

pub use live::{LiveKept, clear_live, keep_live, live_request, save_and_clear_live, save_live_day};
pub use saving::save_day;
pub use shaping::{gunzip, prepare};
pub use staging::stage;
pub use storage::{DATABASE, LIVE_DATABASE, LIVE_STORAGE, RoutesStore, STORAGE};
pub(crate) use storage::{add_details, add_live_work_phone, add_old_days, add_work_phone};

use dispatch_core::server::cache::DataDomain;

pub const COLLECTION: &str = "routes";
/// What a schedule collects by to read today so far, rather than yesterday's finished day
/// and the days before it lacks: an alias of the routes collection. Permanent: schedules
/// store it.
pub const TODAY: &str = "routes_today";
/// The routes, itineraries and packages it keeps.
pub const DOMAIN: DataDomain = DataDomain::new("routes");
/// What shaped a publication's rows. 2 keys tasks by itinerary and keeps their events
/// as triples; 3 keeps each itinerary's work phone, which a publication of 2 has extracted from
/// its stored responses (`work_phones`). A reprocess brings an older publication up to date.
pub const ADAPTER_VERSION: i64 = 3;
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

#[cfg(test)]
#[path = "../tests/backend/mod.rs"]
mod tests;
