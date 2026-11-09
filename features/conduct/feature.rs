//! Conduct: a DSP's writeups of its drivers, each a Corrective Action Notice in a category that
//! climbs its tiers, Verbal to Written to Final, tracked per driver. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    ConductCategory, ConductFinalKind, ConductOverview, ConductPerson, ConductSaved,
    ConductStanding, ConductTier, ConductWriteup, ConductWriteupRow,
};

use dispatch_core::{
    db::{Kind, Migration, OwnMigrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional, perm},
};

pub const FEATURE: Feature = Feature {
    place: 150,
    switch: optional("conduct", "Conduct", &[]),
    permissions: &[
        perm("conduct.use", "Use Conduct", 160),
        perm("conduct.manage", "Manage Conduct", 161).implies(&["conduct.use"]),
    ],
    routes: api::routes::routes,
    // How long a writeup counts toward the next tier, and the categories, on its own page of
    // Settings.
    settings: &[backend::settings::GENERAL, backend::settings::CATEGORIES],
    tables: &[(
        "dsp",
        &["conduct_categories", "conduct_writeups", "conduct_copies"],
    )],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 1,
            name: "conduct",
            apply: Sql(include_str!("migrations/dsp/0001_conduct.sql")),
        }],
    }],
    domains: &[backend::DOMAIN],
    // Its categories' changes and a writeup's deletion, filed under Settings.
    audit: Audit::NONE,
    ..feature("conduct")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        ConductTier,
        ConductFinalKind,
        ConductCategory,
        ConductPerson,
        ConductWriteupRow,
        ConductStanding,
        ConductOverview,
        ConductWriteup,
        ConductSaved,
    )
}
