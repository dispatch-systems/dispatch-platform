pub mod cli;
pub mod contracts;
#[path = "../../features/driver_match/backend/mod.rs"]
pub mod driver_match;
#[path = "../../features/dvic/backend/mod.rs"]
pub mod dvic;
pub mod feature_manifests;
#[path = "../../features/timecard/backend/meals/mod.rs"]
pub mod meals;
#[path = "../../features/routes/backend/mod.rs"]
pub mod routedata;
pub mod routes;
#[path = "../../features/scorecard/backend/mod.rs"]
pub mod scorecard;
#[path = "../../features/uniforms/backend/mod.rs"]
pub mod uniforms;
#[path = "../../features/timecard/backend/punches/mod.rs"]
pub mod workforce;

use dispatch_core::{
    manifest::{self, Registry},
    server::http,
};

/// Everything this build of Dispatch is made of: its collectors and its features.
pub static REGISTRY: Registry = Registry {
    collectors: &[&dispatch_paycom::COLLECTOR, &dispatch_cortex::COLLECTOR],
    features: &[
        &feature_manifests::timecard::FEATURE,
        &feature_manifests::uniforms::FEATURE,
        &feature_manifests::routes::FEATURE,
        &feature_manifests::dvic::FEATURE,
        &feature_manifests::scorecard::FEATURE,
        &feature_manifests::driver_match::FEATURE,
        &feature_manifests::team::FEATURE,
        &feature_manifests::settings::FEATURE,
        &feature_manifests::home::FEATURE,
    ],
};
/// Installs `REGISTRY`, and hands core what a DSP path no route matches asks for. Every
/// entry point calls this before anything reads the registry; calling it again changes
/// nothing.
pub fn install() {
    manifest::install(&REGISTRY);
    http::unmatched_areas(routes::area_permission);
}

/// Core's test support, for this crate's module tests.
#[cfg(test)]
use dispatch_core::testing;
/// Every module test in this crate's test binary, the app's own and, until each feature is a
/// crate of its own, theirs, runs under the app's registry, which holds every part a test
/// names. It is installed before the first test starts, as the app installs it before it
/// serves, so no test can install a smaller one first.
#[cfg(test)]
#[used]
#[unsafe(link_section = ".init_array")]
static INSTALL_BEFORE_TESTS: extern "C" fn() = {
    extern "C" fn install_before_tests() {
        install();
    }
    install_before_tests
};

#[cfg(test)]
#[path = "../tests/backend/agent_calls.rs"]
mod agent_calls;
#[cfg(test)]
#[path = "../tests/backend/agent_catalog.rs"]
mod agent_catalog;
#[cfg(test)]
#[path = "../tests/backend/agent_pages.rs"]
mod agent_pages;
#[cfg(test)]
#[path = "../tests/backend/agent_skill.rs"]
mod agent_skill;
#[cfg(test)]
#[path = "../tests/backend/audit.rs"]
mod audit_areas;
#[cfg(test)]
#[path = "../tests/backend/cache.rs"]
mod cache;
#[cfg(test)]
#[path = "../tests/backend/cache_domains.rs"]
mod cache_domains;
#[cfg(test)]
#[path = "../tests/backend/catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "../tests/backend/collector_registry.rs"]
mod collector_registry;
#[cfg(test)]
#[path = "../tests/backend/collector_storage.rs"]
mod collector_storage;
#[cfg(test)]
#[path = "../tests/backend/connected_app_mail.rs"]
mod connected_app_mail;
#[cfg(test)]
#[path = "../tests/backend/databases.rs"]
mod databases;
#[cfg(test)]
#[path = "../tests/backend/feature_switches.rs"]
mod feature_switches;
#[cfg(test)]
#[path = "../tests/backend/hooks.rs"]
mod hooks;
#[cfg(test)]
#[path = "../tests/backend/job_queue.rs"]
mod job_queue;
#[cfg(test)]
#[path = "../tests/backend/live_results.rs"]
mod live_results;
#[cfg(test)]
#[cfg(feature = "operator-probes")]
#[path = "../tests/backend/probes.rs"]
mod probes;
#[cfg(test)]
#[path = "../tests/backend/registry.rs"]
mod registry_checks;
#[cfg(test)]
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
#[path = "../tests/backend/tables.rs"]
mod tables;
#[cfg(test)]
#[path = "../tests/backend/lib.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod verification;
