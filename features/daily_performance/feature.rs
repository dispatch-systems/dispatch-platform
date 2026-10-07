//! Daily Quality and Day Safety, stored and read independently of posted weekly scorecards.
mod api;
mod backend;
mod mcp;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    DailyPerformanceDataset, DailyPerformanceDay, DailyPerformancePolicy, DailyPerformanceSummary,
};
pub use backend::storage::{DATABASE, DailyPerformanceStore};

use dispatch_core::db::{Migration, Migrations, migrations::Apply::Sql};
use dispatch_core::manifest::{Audit, Feature, Switch, feature, perm};
use dispatch_core::tenancy::api::audit::AuditArea::Collections;

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "daily_performance",
        label: "Daily Performance",
        requires: &["daily_performance"],
    }),
    permissions: &[
        perm("daily_performance.view", "View Daily Performance", 100),
        perm(
            "daily_performance.collect",
            "Collect Daily Performance",
            101,
        )
        .implies(&["daily_performance.view"]),
        perm("daily_performance.manage", "Manage Daily Performance", 102)
            .implies(&["daily_performance.view"]),
    ],
    domains: &[backend::storage::DOMAIN],
    routes: api::routes::routes,
    keeps: &[&backend::keeper::DailyPerformanceKeeper],
    tables: &[(
        "daily_performance",
        &[
            "settings",
            "daily_publications",
            "daily_datasets",
            "daily_rows",
        ],
    )],
    migrations: &[Migrations {
        kind: backend::storage::DATABASE,
        list: &[Migration {
            id: 1,
            name: "baseline",
            apply: Sql(include_str!(
                "migrations/daily_performance/0001_baseline.sql"
            )),
        }],
    }],
    audit: Audit {
        areas: &[("daily_performance.", Collections)],
        ..Audit::NONE
    },
    mcp: mcp::MCP,
    people: &[&backend::people::Drivers],
    ..feature("daily_performance")
};
