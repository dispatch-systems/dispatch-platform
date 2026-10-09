//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{DvicInspection, DvicInspections, DvicReport, DvicStatus, DvicWeek};
/// What the app uses: its storage and its database.
pub use backend::{DATABASE, DvicStore};

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Commands, Feature, feature, optional, perm, tab},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    place: 50,
    switch: optional("dvic", "DVIC", &["dvic"]),
    subfeatures: &[tab("dvic.day", "Day"), tab("dvic.week", "Week")],
    permissions: &[
        perm("dvic.view", "View DVIC", 40),
        perm("dvic.collect", "Collect DVIC", 41).implies(&["dvic.view"]),
        perm("dvic.manage", "Manage DVIC", 42).implies(&["dvic.view"]),
    ],
    routes: api::routes::routes,
    live: &["dvic.view"],
    keeps: &[&backend::keeper::Dvic],
    tables: &[(
        "dvic",
        &[
            "dvic_reports",
            "dvic_revisions",
            "dvic_inspections",
            "dvic_weeks",
            "dvic_runs",
            "dvic_hidden_drivers",
        ],
    )],
    migrations: &[Migrations {
        kind: DATABASE,
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!("migrations/dvic/0001_baseline.sql")),
            },
            Migration {
                id: 2,
                name: "hidden_drivers",
                apply: Sql(include_str!("migrations/dvic/0002_hidden_drivers.sql")),
            },
            Migration {
                id: 3,
                name: "verified_scope",
                apply: Code(backend::add_verified_scope),
            },
        ],
    }],
    domains: &[backend::DOMAIN],
    audit: Audit {
        areas: &[("dvic.", Collections)],
        ..Audit::NONE
    },
    commands: Some(Commands {
        prefix: "dvic-",
        run: backend::cli::run,
    }),
    mcp: mcp::MCP,
    people: &[&backend::people::Drivers],
    ..feature("dvic")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        DvicReport,
        DvicWeek,
        DvicStatus,
        DvicInspection,
        DvicInspections,
    )
}
