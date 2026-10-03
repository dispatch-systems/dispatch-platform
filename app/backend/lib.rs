#[path = "../../core/accounts/backend/mod.rs"]
pub mod accounts;
#[path = "../../core/mcp/backend/mod.rs"]
pub mod agents;
#[path = "../../core/tenancy/backend/audit.rs"]
pub mod audit;
#[path = "../../core/collection/backend/browser/mod.rs"]
pub mod browsers;
#[path = "cli.rs"]
pub mod cli;
#[path = "../../core/collection/backend/registry.rs"]
pub mod collectors;
#[path = "../../core/foundation/backend/config/mod.rs"]
pub mod config;
#[path = "contracts.rs"]
pub mod contracts;
#[path = "../../core/foundation/backend/crypto.rs"]
pub mod crypto;
#[path = "../../core/db/backend/mod.rs"]
pub mod db;
#[path = "../../features/driver_match/backend/mod.rs"]
pub mod driver_match;
#[path = "../../features/dvic/backend/mod.rs"]
pub mod dvic;
#[path = "../../core/foundation/backend/error.rs"]
pub mod error;
#[path = "feature_manifests.rs"]
pub mod feature_manifests;
#[path = "../../core/tenancy/backend/catalog.rs"]
pub mod features;
#[path = "../../features/home/backend/mod.rs"]
pub mod home;
#[path = "../../core/server/backend/http/mod.rs"]
pub mod http;
#[path = "../../core/collection/backend/metrics.rs"]
pub mod job_metrics;
#[path = "../../core/collection/backend/jobs/mod.rs"]
pub mod jobs;
#[path = "../../core/collection/backend/live.rs"]
pub mod live_collection;
#[path = "../../core/server/backend/live.rs"]
pub mod live_updates;
#[path = "../../core/server/backend/mail/mod.rs"]
pub mod mail;
#[path = "../../core/manifest/backend/mod.rs"]
pub mod manifest;
#[path = "../../features/timecard/backend/meals/mod.rs"]
pub mod meals;
#[path = "../../core/foundation/backend/names.rs"]
pub mod names;
#[path = "../../core/foundation/backend/observability.rs"]
pub mod observability;
#[path = "../../core/server/backend/operations.rs"]
pub mod operations;
#[path = "../../core/server/backend/presence.rs"]
pub mod presence;
#[path = "../../core/server/backend/http/proxy.rs"]
pub mod proxy;
#[path = "../../core/server/backend/cache.rs"]
pub mod read_cache;
#[path = "../../core/tenancy/backend/roles.rs"]
pub mod roles;
#[path = "../../features/routes/backend/mod.rs"]
pub mod routedata;
#[path = "../../core/collection/backend/schedules.rs"]
pub mod schedules;
#[path = "../../features/scorecard/backend/mod.rs"]
pub mod scorecard;
#[path = "../../core/server/backend/state.rs"]
mod state;
#[path = "../../core/tenancy/backend/dsps.rs"]
pub mod tenants;
#[path = "../../features/uniforms/backend/mod.rs"]
pub mod uniforms;
#[path = "../../core/foundation/backend/validate.rs"]
pub mod validate;
#[path = "../../core/foundation/backend/weeks.rs"]
pub mod weeks;
#[path = "../../core/foundation/backend/wire.rs"]
pub mod wire;
#[path = "../../features/timecard/backend/punches/mod.rs"]
pub mod workforce;

pub use error::{Code, Error, Result, ensure};
pub use state::{State, cancelled, supervise};

/// Everything this build of Dispatch is made of: its collectors and its features.
pub static REGISTRY: manifest::Registry = manifest::Registry {
    collectors: &[
        &collectors::paycom::COLLECTOR,
        &collectors::cortex::COLLECTOR,
    ],
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
/// Installs `REGISTRY`. Every entry point calls this before anything reads the registry;
/// calling it again changes nothing.
pub fn install() {
    manifest::install(&REGISTRY);
}

#[cfg(test)]
#[path = "../tests/backend/audit.rs"]
mod audit_areas;
#[cfg(test)]
#[path = "../tests/backend/cache.rs"]
mod cache;
#[cfg(test)]
#[path = "../tests/backend/catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "../tests/backend/databases.rs"]
mod databases;
#[cfg(test)]
#[path = "../tests/backend/maintenance.rs"]
mod maintenance;
#[cfg(test)]
#[path = "../tests/backend/request_log.rs"]
mod request_log;
#[cfg(test)]
#[path = "../tests/backend/verification.rs"]
mod verification;
#[cfg(test)]
mod tests {
    #[test]
    fn every_collection_has_exactly_one_keeper() {
        super::REGISTRY.check();
    }
}
