//! The job engine, schedules, live results, metrics, the collection browser and the
//! registry of collectors.
#[path = "../api/mod.rs"]
pub mod api;
pub mod browser;
pub mod jobs;
pub mod live;
pub mod metrics;
pub mod registry;
pub mod schedules;
