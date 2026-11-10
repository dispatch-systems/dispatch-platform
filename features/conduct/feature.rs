//! Conduct: a DSP's writeups of its drivers, each a Corrective Action Notice in a category that
//! climbs its tiers, Verbal to Written to Final, and steps back down in time, tracked per
//! driver. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    ConductBackup, ConductCategory, ConductFinalKind, ConductOverview, ConductPerson, ConductSaved,
    ConductStanding, ConductStepDown, ConductStepKind, ConductTier, ConductUnit, ConductWriteup,
    ConductWriteupRow,
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
    // The categories, with their tiers' step downs, and backing signed copies up, on its own
    // page of Settings.
    settings: &[backend::settings::CATEGORIES, backend::settings::BACKUP],
    tables: &[(
        "dsp",
        &[
            "conduct_categories",
            "conduct_writeups",
            "conduct_copies",
            "conduct_backups",
            "conduct_backup_folders",
        ],
    )],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[
            Migration {
                id: 1,
                name: "conduct",
                apply: Sql(include_str!("migrations/dsp/0001_conduct.sql")),
            },
            Migration {
                id: 2,
                name: "conduct_backups",
                apply: Sql(include_str!("migrations/dsp/0002_conduct_backups.sql")),
            },
        ],
    }],
    // Every minute, the signed copies not yet backed up.
    maintenance: &[backend::backups::MAINTENANCE],
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
        ConductUnit,
        ConductStepKind,
        ConductStepDown,
        ConductCategory,
        ConductPerson,
        ConductWriteupRow,
        ConductStanding,
        ConductOverview,
        ConductWriteup,
        ConductSaved,
        ConductBackup,
    )
}
