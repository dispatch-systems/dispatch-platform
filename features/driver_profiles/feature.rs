//! Driver Profiles: everyone Amazon lists for the DSP, with their contacts, roles and photos,
//! collected from Cortex's associates page, and Driver Match, one code per person across
//! every source that names them. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Driver, DriverActivity, DriverCounts, DriverDay, DriverDetails, DriverEvent, DriverEventKind,
    DriverEvidence, DriverEvidenceKind, DriverId, DriverLink, DriverMatch, DriverPair,
    DriverStrength, RosterStatus,
};
/// What the app and Timecard, which depends on it, use: the roster's storage and the cache
/// domain its collections change; Driver Match's storage, the cache domain its decisions
/// change, the links the meal-break page saved before it, and its codes' shape.
pub use backend::{
    DOMAIN,
    driver_match::{DOMAIN as DRIVERS, DriverMatchStore, DriverSources, LINKS, valid_code},
    storage::DriverProfilesStore,
};

use dispatch_core::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional, perm, sub},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    place: 70,
    switch: optional("driver_profiles", "Driver Profiles", &["associates"]),
    // Driver Match's tab is one of Settings', not a page of its own: it matches Paycom's
    // employees to the drivers the roster, routes and other collections name.
    subfeatures: &[sub("driver_match", "Driver Match")
        .permissions(&[perm("driver_match.manage", "Manage Driver Match", 60)])
        .requires(&["timecards", "routes"])],
    permissions: &[perm(
        "driver_profiles.collect",
        "Collect Driver Profiles",
        59,
    )],
    routes: api::routes::routes,
    live: &["driver_profiles.collect"],
    keeps: &[&backend::keeper::Roster],
    tables: &[
        (
            "driver_profiles",
            &["associates", "photos", "settings", "roster_runs"],
        ),
        ("platform", &["driver_codes"]),
        ("dsp", &["people", "person_ids", "people_apart"]),
    ],
    migrations: &[
        Migrations {
            kind: backend::storage::DATABASE,
            list: &[Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/driver_profiles/0001_baseline.sql")),
            }],
        },
        Migrations {
            kind: Kind::PLATFORM,
            list: &[Migration {
                id: 10,
                name: "driver_codes",
                apply: Sql(include_str!("migrations/platform/0010_driver_codes.sql")),
            }],
        },
        Migrations {
            kind: Kind::DSP,
            list: &[Migration {
                id: 7,
                name: "driver_match",
                apply: Sql(include_str!("migrations/dsp/0007_driver_match.sql")),
            }],
        },
    ],
    domains: &[backend::DOMAIN, backend::driver_match::DOMAIN],
    maintenance: &[
        backend::maintenance::MAINTENANCE,
        backend::driver_match::maintenance::MAINTENANCE,
    ],
    after_collection: Some(backend::driver_match::maintenance::after_collection),
    // Driver Match's events name drivers by their codes, and the log puts today's names to
    // them.
    audit: Audit {
        areas: &[("driver_profiles.", Collections)],
        subjects: &["driver"],
        names: Some(backend::driver_match::name_driver_events),
    },
    people: &[&backend::people::Associates],
    ..feature("driver_profiles")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        RosterStatus,
        DriverLink,
        DriverStrength,
        DriverEvidenceKind,
        DriverEventKind,
        DriverId,
        Driver,
        DriverCounts,
        DriverEvidence,
        DriverPair,
        DriverMatch,
        DriverActivity,
        DriverDay,
        DriverEvent,
        DriverDetails,
    )
}
