//! Daily Quality and Day Safety, stored and read independently of posted weekly scorecards.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    DailyPerformanceDataset, DailyPerformanceDay, DailyPerformancePolicy, DailyPerformanceSummary,
};
pub use backend::storage::{DATABASE, DailyPerformanceStore};

use dispatch_core::db::{Migration, Migrations, migrations::Apply::Sql};
use dispatch_core::manifest::{Audit, Feature, feature, optional, perm};
use dispatch_core::tenancy::api::audit::AuditArea::Collections;

pub const FEATURE: Feature = Feature {
    place: 80,
    switch: optional(
        "daily_performance",
        "Daily Performance",
        &["daily_performance"],
    ),
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
    people: &[&backend::people::Drivers],
    ..feature("daily_performance")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        DailyPerformanceSummary,
        DailyPerformanceDay,
        DailyPerformanceDataset,
        DailyPerformancePolicy,
    )
}
