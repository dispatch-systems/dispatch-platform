//! Driver Profiles: everyone Amazon lists for the DSP, with their contacts, roles and photos,
//! collected from Cortex's associates page. Driver Match reads them as it reads every
//! feature's people. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::RosterStatus;
/// What the app uses: the roster's storage and the cache domain its collections change.
pub use backend::{DOMAIN, storage::DriverProfilesStore};

use dispatch_core::{
    db::{Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional, perm},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    place: 70,
    // It began as Driver Match's home, so a DSP that had Driver Match on before gets it.
    switch: optional("driver_profiles", "Driver Profiles", &["associates"])
        .starts_on_with(&["driver_match"]),
    permissions: &[perm(
        "driver_profiles.collect",
        "Collect Driver Profiles",
        59,
    )],
    routes: api::routes::routes,
    live: &["driver_profiles.collect"],
    keeps: &[&backend::keeper::Roster],
    tables: &[(
        "driver_profiles",
        &["associates", "photos", "settings", "roster_runs"],
    )],
    migrations: &[Migrations {
        kind: backend::storage::DATABASE,
        list: &[Migration {
            id: 1,
            name: "baseline",
            apply: Sql(include_str!("migrations/driver_profiles/0001_baseline.sql")),
        }],
    }],
    domains: &[backend::DOMAIN],
    maintenance: &[backend::maintenance::MAINTENANCE],
    audit: Audit {
        areas: &[("driver_profiles.", Collections)],
        ..Audit::NONE
    },
    people: &[&backend::people::PEOPLE],
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_driver_profiles_contract::AmazonRoster,
        &backend::exposed::EXPOSED,
        "amazon_roster",
        "TODO(scaffold): what it offers, in a sentence.",
    )],
    ..feature("driver_profiles")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, RosterStatus)
}
