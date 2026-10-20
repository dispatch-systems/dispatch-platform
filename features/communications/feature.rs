//! Communications: TODO(scaffold): what it does for a DSP, in a sentence. Its README says more.
mod api;
mod backend;
mod integrations;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::CommunicationsSummary;
pub use backend::storage::{CommunicationsItem, CommunicationsStore};

use dispatch_core::db::{Kind, Migration, OwnMigrations, migrations::Apply::Sql};
use dispatch_core::manifest::{Feature, feature, optional, perm};

pub const FEATURE: Feature = Feature {
    place: 140,
    switch: optional("communications", "Communications", &[]),
    permissions: &[perm("communications.view", "View Communications", 140)],
    routes: api::routes::routes,
    tables: &[("dsp", &["communications_items"])],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 1,
            name: "communications",
            apply: Sql(include_str!("migrations/dsp/0001_communications.sql")),
        }],
    }],
    depends_on: &["slack"],
    integrations: &[integrations::messages::INTEGRATION],
    ..feature("communications")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, CommunicationsSummary)
}
