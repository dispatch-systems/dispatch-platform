//! Daily Quality and Day Safety, stored and read independently of posted weekly scorecards.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{DailyPerformanceDataset, DailyPerformanceDay, DailyPerformanceSummary};
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
    // Which dates its collection rechecks, drawn with it on Settings' Collectors page.
    settings: &[backend::settings::COLLECTION],
    upgrade_storage: Some(backend::settings::upgrade),
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
        list: &[
            Migration {
                id: 1,
                name: "baseline",
                apply: Sql(include_str!(
                    "migrations/daily_performance/0001_baseline.sql"
                )),
            },
            Migration {
                id: 2,
                name: "days_per_job",
                apply: Sql(include_str!(
                    "migrations/daily_performance/0002_days_per_job.sql"
                )),
            },
        ],
    }],
    audit: Audit {
        areas: &[("daily_performance.", Collections)],
        ..Audit::NONE
    },
    people: &[&backend::people::PEOPLE],
    // What it offers every other feature and agent tool, through its contract.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_daily_performance_contract::DailyMetrics,
        &backend::exposed::EXPOSED,
        "daily_metrics",
        "A driver's Daily Quality and Day Safety day by day, the packages they returned and their safety events, and the days collected.",
    )],
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
    )
}
