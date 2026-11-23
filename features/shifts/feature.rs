//! Schedules: every shift Amazon's Scheduling lists for the DSP's drivers, collected week by
//! week from Cortex, with every change to one kept. Its README says more.
mod api;
mod backend;

/// What the app uses: the schedules' storage and the cache domain its collections change.
pub use backend::{DOMAIN, storage::ShiftsStore};

use dispatch_core::{
    db::{Migration, Migrations, migrations::Apply::Sql},
    manifest::{Feature, feature, optional, perm},
};

pub const FEATURE: Feature = Feature {
    place: 180,
    switch: optional("shifts", "Schedules", &["shifts"]),
    permissions: &[perm("shifts.collect", "Collect Schedules", 181)],
    routes: api::routes::routes,
    live: &["shifts.collect"],
    keeps: &[&backend::keeper::Shifts],
    tables: &[(
        "shifts",
        &["shift_runs", "shift_assignments", "shift_changes"],
    )],
    migrations: &[Migrations {
        kind: backend::storage::DATABASE,
        list: &[Migration {
            id: 1,
            name: "baseline",
            apply: Sql(include_str!("migrations/shifts/0001_baseline.sql")),
        }],
    }],
    domains: &[backend::DOMAIN],
    ..feature("shifts")
};
