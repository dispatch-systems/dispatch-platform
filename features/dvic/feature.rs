//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{DvicInspection, DvicInspections, DvicReport, DvicStatus, DvicWeek};
/// What the app uses: its storage and its database.
pub use backend::{DATABASE, DvicStore};
/// What its integration tests check beside the storage: the operator's commands, the drivers
/// they hide, and the weeks a request names.
pub use backend::{cli, hidden, weeks_ending};
/// What agents can ask of it, which the app's tests ask directly.
pub use mcp::views::dvic;

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Commands, Feature, Switch, feature, perm, tab},
    tenancy::api::audit::AuditArea::Collections,
};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "dvic",
        label: "DVIC",
        requires: &["dvic"],
    }),
    tabs: &[tab("dvic.day", "Day"), tab("dvic.week", "Week")],
    permissions: &[
        perm("dvic.view", "View DVIC", 40),
        perm("dvic.collect", "Collect DVIC", 41).implies(&["dvic.view"]),
        perm("dvic.manage", "Manage DVIC", 42).implies(&["dvic.view"]),
    ],
    routes: api::routes::routes,
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
        run: cli::run,
    }),
    mcp: mcp::MCP,
    people: &[&backend::people::Drivers],
    ..feature("dvic")
};
