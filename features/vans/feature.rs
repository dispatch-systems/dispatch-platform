//! Vans: every vehicle Amazon's fleet lists for the DSP, with the name the DSP gives it,
//! collected from Cortex's Vehicles page, so a VIN reads as the DSP's own van. Its README says
//! more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::FleetStatus;
/// What the app uses: the fleet's storage and the cache domain its collections change.
pub use backend::{DOMAIN, storage::VansStore};

use dispatch_core::{
    db::{Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional, perm},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    place: 130,
    switch: optional("vans", "Vans", &["vans"]),
    permissions: &[perm("vans.collect", "Collect Vans", 131)],
    routes: api::routes::routes,
    // Its fleet bar and its schedules, on its own Settings page.
    settings: &[backend::settings::FLEET],
    live: &["vans.collect"],
    keeps: &[&backend::keeper::Fleet],
    tables: &[("vans", &["vans", "fleet_runs"])],
    migrations: &[Migrations {
        kind: backend::storage::DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/vans/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "drop_settings",
                apply: Sql(include_str!("migrations/vans/0002_drop_settings.sql")),
            },
        ],
    }],
    domains: &[backend::DOMAIN],
    audit: Audit {
        areas: &[("vans.", Collections)],
        ..Audit::NONE
    },
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_vans_contract::AmazonFleet,
        &backend::exposed::EXPOSED,
        "amazon_fleet",
        "What Amazon's fleet says of each of the DSP's vans by VIN: the name the DSP gives it.",
    )],
    ..feature("vans")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, FleetStatus)
}
