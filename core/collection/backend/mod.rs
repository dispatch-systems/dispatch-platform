//! The job engine, schedules, live results, the live readers, metrics, the collection browser
//! and the registry of collectors.
#[path = "../api/mod.rs"]
pub mod api;
pub mod browser;
pub mod jobs;
pub mod live;
pub mod metrics;
pub mod readers;
pub mod registry;
pub mod rules;
pub mod schedules;
pub mod settings;
pub mod verification;
