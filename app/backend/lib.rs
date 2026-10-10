// A build that leaves features out drops the tests that need them, and what only they use.
#![cfg_attr(all(test, not(feature = "default")), allow(dead_code, unused_imports))]

pub mod cli;
mod features;
pub mod routes;

use dispatch_core::{
    manifest::{self, Registry},
    server::http,
};

/// Everything this build of Dispatch is made of: its collectors, its features and the MCP. Each
/// feature is a Cargo feature of the app, which a build may leave out.
pub static REGISTRY: Registry = Registry {
    collectors: &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
    features: features::FEATURES,
    agents: Some(&dispatch_mcp::AGENTS),
};
/// Installs `REGISTRY`, and hands core what a DSP path no route matches asks for. Every
/// entry point, and every one of the app's tests, calls this before anything reads the
/// registry; calling it again changes nothing.
pub fn install() {
    #[cfg(feature = "default")]
    manifest::install(&REGISTRY);
    // A build that leaves features out lacks what only they bring, such as a collection's
    // keeper, so only its tests run it, as each owner's tests run theirs.
    #[cfg(not(feature = "default"))]
    manifest::install_partial(&REGISTRY);
    http::unmatched_areas(routes::area_permission);
}

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;

#[cfg(test)]
#[path = "../tests/backend/agent_calls.rs"]
mod agent_calls;
#[cfg(test)]
#[path = "../tests/backend/agent_toolbox.rs"]
mod agent_toolbox;
#[cfg(test)]
#[cfg(all(feature = "timecard", feature = "dvic"))]
#[path = "../tests/backend/audit.rs"]
mod audit_areas;
#[cfg(test)]
#[cfg(all(
    feature = "timecard",
    feature = "routes",
    feature = "dvic",
    feature = "weekly_scorecard"
))]
#[path = "../tests/backend/cache.rs"]
mod cache;
#[cfg(test)]
#[cfg(feature = "timecard")]
#[path = "../tests/backend/cache_domains.rs"]
mod cache_domains;
#[cfg(test)]
#[path = "../tests/backend/catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "../tests/backend/collector_registry.rs"]
mod collector_registry;
#[cfg(test)]
#[cfg(feature = "timecard")]
#[path = "../tests/backend/collector_storage.rs"]
mod collector_storage;
#[cfg(test)]
#[path = "../tests/backend/contracts.rs"]
mod contracts;
#[cfg(test)]
#[cfg(feature = "default")]
#[path = "../tests/backend/databases.rs"]
mod databases;
#[cfg(test)]
#[path = "../tests/backend/export.rs"]
mod export;
#[cfg(test)]
#[path = "../tests/backend/feature_switches.rs"]
mod feature_switches;
#[cfg(test)]
#[path = "../tests/backend/job_queue.rs"]
mod job_queue;
#[cfg(test)]
#[cfg(feature = "timecard")]
#[path = "../tests/backend/live_results.rs"]
mod live_results;
#[cfg(test)]
#[cfg(all(feature = "operator-probes", feature = "default"))]
#[path = "../tests/backend/probes.rs"]
mod probes;
#[cfg(test)]
#[cfg(feature = "default")]
#[path = "../tests/backend/registry.rs"]
mod registry_checks;
#[cfg(test)]
#[cfg(feature = "timecard")]
#[path = "../tests/backend/request_log.rs"]
mod request_log;
#[cfg(test)]
#[path = "../tests/backend/retryable_codes.rs"]
mod retryable_codes;
#[cfg(test)]
#[path = "../tests/backend/schedules.rs"]
mod schedule_runs;
#[cfg(test)]
#[path = "../tests/backend/schema.rs"]
mod schema;
#[cfg(test)]
#[path = "../tests/backend/snapshot.rs"]
mod snapshot;
#[cfg(test)]
#[cfg(feature = "default")]
#[path = "../tests/backend/tables.rs"]
mod tables;
#[cfg(test)]
#[cfg(feature = "team")]
#[path = "../tests/backend/verification.rs"]
mod verification;
