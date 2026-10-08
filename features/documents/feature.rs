//! Documents: the DSP's shared folders, Docs and Sheets, kept in a Google account the DSP
//! connects. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    AccountKind, ConnectionStatus, DocumentsConnection, DocumentsOverview, GoogleSignIn,
};

use dispatch_core::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, Switch, feature, perm},
    tenancy::api::audit::AuditArea::Connections,
};

pub const FEATURE: Feature = Feature {
    switch: Some(Switch {
        id: "documents",
        label: "Documents",
        requires: &[],
    }),
    permissions: &[
        perm("documents.use", "Use Documents", 110),
        perm("documents.manage", "Manage Documents", 111).implies(&["documents.use"]),
    ],
    routes: api::routes::routes,
    tables: &[(
        "dsp",
        &["documents_connection", "documents_connect_requests"],
    )],
    migrations: &[Migrations {
        kind: Kind::DSP,
        list: &[Migration {
            id: 9,
            name: "documents",
            apply: Sql(include_str!("migrations/dsp/0009_documents.sql")),
        }],
    }],
    maintenance: &[backend::maintenance::MAINTENANCE],
    audit: Audit {
        areas: &[("documents.", Connections)],
        ..Audit::NONE
    },
    ..feature("documents")
};
