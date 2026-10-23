//! Driver Profiles: everyone Amazon lists for the DSP, with their contacts, roles and photos,
//! collected from Cortex's associates page. Driver Match reads them as it reads every
//! feature's people. Its README says more.
mod api;
mod backend;

/// What the app uses: the roster's storage and the cache domain its collections change.
pub use backend::{DOMAIN, storage::DriverProfilesStore};

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
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
    tables: &[("driver_profiles", &["associates", "photos", "roster_runs"])],
    migrations: &[Migrations {
        kind: backend::storage::DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/driver_profiles/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "drop_settings",
                apply: Sql(include_str!(
                    "migrations/driver_profiles/0002_drop_settings.sql"
                )),
            },
            Migration {
                id: 3,
                name: "photo_etags",
                apply: Code(backend::storage::add_photo_etags),
            },
        ],
    }],
    domains: &[backend::DOMAIN],
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
        "What Amazon's roster says of each associate: status, roles, qualifications, license expiry.",
    )],
    ..feature("driver_profiles")
};
