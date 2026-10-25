//! Phones: a DSP's dispatch phones, each relaying the texts and pictures its number receives
//! and sends to Dispatch, and sending Dispatch's replies from it. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{PhonePairing, PhoneRow, Phones};

use dispatch_core::{
    db::{Kind, Migration, OwnMigrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional},
    tenancy::api::audit::AuditArea::Connections,
};

pub const FEATURE: Feature = Feature {
    place: 150,
    switch: optional("phones", "Phones", &[]),
    // Pairing and removing phones is managing the DSP's connections; reading their texts is
    // whoever reads them through `texting`, under their own permissions.
    routes: api::routes::routes,
    domains: &[backend::storage::DOMAIN],
    tables: &[(
        "dsp",
        &[
            "phones_devices",
            "phones_pairings",
            "phones_texts",
            "phones_files",
            "phones_change",
        ],
    )],
    own_migrations: &[OwnMigrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 1,
            name: "phones",
            apply: Sql(include_str!("migrations/dsp/0001_phones.sql")),
        }],
    }],
    dsp_connection: true,
    // Letting go of pictures never sent and codes expired, hourly.
    maintenance: &[backend::tidy::MAINTENANCE],
    demo: Some(backend::demo::demo),
    audit: Audit {
        areas: &[("phones.", Connections)],
        ..Audit::NONE
    },
    // The texts its phones relay, and sending one, for any feature.
    exposes: &[dispatch_core::expose!(
        dyn dispatch_phones_contract::Texting,
        &backend::exposed::EXPOSED,
        "texting",
        "The texts and pictures a DSP's dispatch phones relay, and sending one from its number.",
    )],
    ..feature("phones")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(cfg, Phones, PhoneRow, PhonePairing)
}
