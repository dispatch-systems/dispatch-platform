//! DVIC: vehicle inspections, from the weekly reports Cortex publishes.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    DvicCell, DvicCoverage, DvicDaySummary, DvicDriverSummary, DvicInspection,
    DvicInspectionDetail, DvicInspectionPage, DvicInspections, DvicReport, DvicStatus, DvicSummary,
    DvicWeek,
};
/// What the app uses: its storage and its database.
pub use backend::{DATABASE, DvicDriver, DvicStore};

use dispatch_core::{
    db::{
        Migration, Migrations,
        migrations::Apply::{Code, Sql},
    },
    manifest::{Audit, Commands, Feature, feature, optional, perm, tab},
    server::cache::{Cached, DataDomain, Evicted::By},
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
    // When its reports are collected, on its own Settings page.
    settings: &[backend::settings::COLLECTION],
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
    cached: &[Cached {
        read: backend::CACHED,
        evicted: By(&[backend::DOMAIN, DataDomain::TENANT]),
    }],
    audit: Audit {
        areas: &[("dvic.", Collections)],
        ..Audit::NONE
    },
    commands: Some(Commands {
        prefix: "dvic-",
        run: backend::cli::run,
    }),
    people: &[&backend::people::PEOPLE],
    // Agent tools and other features read its short inspections without depending on it.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_dvic_contract::ShortInspections,
        &backend::reader::READER,
        "short_inspections",
        "The pre-trip inspections shorter than the minimum, who made them, and the days collected.",
    )],
    ..feature("dvic")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        DvicReport,
        DvicWeek,
        DvicCoverage,
        DvicStatus,
        DvicInspection,
        DvicInspections,
        DvicCell,
        DvicDriverSummary,
        DvicDaySummary,
        DvicSummary,
        DvicInspectionPage,
        DvicInspectionDetail,
    )
}
