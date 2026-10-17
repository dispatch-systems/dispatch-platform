//! Driver Match: one code for each person, whichever of the DSP's features name them and
//! however each writes them. Every DSP has it. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Driver, DriverActivity, DriverCounts, DriverDays, DriverDetails, DriverEvent, DriverEventKind,
    DriverEvidence, DriverEvidenceKind, DriverId, DriverLink, DriverMatch, DriverPair, DriverRole,
    DriverStrength,
};
/// What the app uses: its storage, what one pass reads, and its codes' shape. Other features
/// ask core's directory who an ID is.
pub use backend::{DriverMatchStore, DriverSources, valid_code};

use dispatch_core::{
    db::{Kind, Migration, Migrations, OwnMigrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, mandatory, perm},
};

pub const FEATURE: Feature = Feature {
    place: 75,
    // Every DSP has it: what the DSP's features name, it tells apart. Its one screen is a tab
    // of Settings.
    switch: mandatory("driver_match", "Driver Match"),
    permissions: &[perm("driver_match.manage", "Manage Driver Match", 60)],
    routes: api::routes::routes,
    tables: &[
        ("platform", &["driver_codes"]),
        ("dsp", &["people", "person_ids", "people_apart"]),
    ],
    migrations: &[
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
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[
            Migration {
                id: 1,
                name: "open_sources",
                apply: Sql(include_str!("migrations/dsp/0001_open_sources.sql")),
            },
            Migration {
                id: 2,
                name: "meal_break_links",
                apply: Sql(include_str!("migrations/dsp/0002_meal_break_links.sql")),
            },
        ],
    }],
    maintenance: &[backend::maintenance::MAINTENANCE],
    after_collection: Some(backend::maintenance::after_collection),
    // Its events name drivers by their codes, and the log puts today's names to them.
    audit: Audit {
        subjects: &["driver"],
        names: Some(backend::name_driver_events),
        ..Audit::NONE
    },
    directory: Some(&backend::Directory),
    ..feature("driver_match")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        DriverLink,
        DriverStrength,
        DriverEvidenceKind,
        DriverEventKind,
        DriverRole,
        DriverId,
        Driver,
        DriverCounts,
        DriverEvidence,
        DriverPair,
        DriverMatch,
        DriverActivity,
        DriverDays,
        DriverEvent,
        DriverDetails,
    )
}
