//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
use crate::dvic;
use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Commands, Feature, Switch, feature, perm, tab},
    tenancy::api::audit::AuditArea::Collections,
};

#[path = "api/routes.rs"]
mod api;
#[path = "mcp/mod.rs"]
pub mod mcp;

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
    routes: api::routes,
    keeps: &[&crate::dvic::keeper::Dvic],
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
        kind: dvic::DATABASE,
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
                apply: Code(dvic::add_verified_scope),
            },
        ],
    }],
    domains: &[dvic::DOMAIN],
    audit: Audit {
        areas: &[("dvic.", Collections)],
        ..Audit::NONE
    },
    commands: Some(Commands {
        prefix: "dvic-",
        run: dvic::cli::run,
    }),
    mcp: mcp::MCP,
    people: &[&dvic::people::Drivers],
    ..feature("dvic")
};
