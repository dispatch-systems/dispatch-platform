//! Driver Match: one code per driver across Paycom and every Amazon source.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    Driver, DriverActivity, DriverCounts, DriverDay, DriverDetails, DriverEvent, DriverEventKind,
    DriverEvidence, DriverEvidenceKind, DriverId, DriverLink, DriverMatch, DriverPair,
    DriverStrength,
};
/// What Timecard, which depends on it, and the app use: its storage, the cache domain its
/// decisions change, the links the meal-break page saved before it, and its codes' shape.
pub use backend::{DOMAIN, DriverMatchStore, DriverSources, LINKS, valid_code};

use dispatch_core::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, Switch, feature, perm},
};

pub const FEATURE: Feature = Feature {
    place: 70,
    // A tab of Settings, not a page of its own: it matches Paycom's employees to the
    // drivers Amazon's routes and other collections name.
    switch: Some(Switch {
        id: "driver_match",
        label: "Driver Match",
        requires: &["timecards", "routes"],
    }),
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
    domains: &[DOMAIN],
    maintenance: &[backend::maintenance::MAINTENANCE],
    after_collection: Some(backend::maintenance::after_collection),
    // Its events name drivers by their codes, and the log puts today's names to them.
    audit: Audit {
        subjects: &["driver"],
        names: Some(backend::name_driver_events),
        ..Audit::NONE
    },
    mcp: mcp::MCP,
    ..feature("driver_match")
};
