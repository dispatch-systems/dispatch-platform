//! Documents: the DSP's shared folders, Docs and Sheets, kept in a Google account the DSP
//! connects. Its README says more.
mod api;
mod backend;

/// What its API answers with, which the app writes to TypeScript.
pub use api::types::{
    AccountKind, ConnectionStatus, DocumentsAccount, DocumentsConnection, DocumentsFolder,
    DocumentsItem, DocumentsOverview, DriveFile, DriveStorage, FolderStep, GoogleConnected,
    GoogleSignIn, ItemKind, MySharing, NewKind, PickerKeys, PickerSetup, SharingState,
};

use dispatch_core::{
    db::{Kind, Migration, Migrations, migrations::Apply::Sql},
    manifest::{Audit, Feature, feature, optional, perm},
    tenancy::api::audit::AuditArea::Connections,
};

pub const FEATURE: Feature = Feature {
    place: 90,
    switch: optional("documents", "Documents", &[]),
    // Everyone who uses Documents works in it alike. Its Google account is one of the DSP's
    // own, managed on Settings' DSP Connections, so roles that managed Documents now use it.
    permissions: &[perm("documents.use", "Use Documents", 110)],
    retired_identifiers: &[("documents.manage", "documents.use")],
    dsp_connection: true,
    routes: api::routes::routes,
    tables: &[(
        "dsp",
        &[
            "documents_connection",
            "documents_connect_requests",
            "documents_files",
            "documents_people",
        ],
    )],
    migrations: &[Migrations {
        kind: Kind::DSP,
        list: &[
            Migration {
                id: 9,
                name: "documents",
                apply: Sql(include_str!("migrations/dsp/0009_documents.sql")),
            },
            Migration {
                id: 10,
                name: "documents_files",
                apply: Sql(include_str!("migrations/dsp/0010_documents_files.sql")),
            },
            Migration {
                id: 11,
                name: "documents_people",
                apply: Sql(include_str!("migrations/dsp/0011_documents_people.sql")),
            },
        ],
    }],
    // Google's sign-in client, and its file picker's keys.
    settings: &[
        &["GOOGLE_CLIENT_ID", "GOOGLE_CLIENT_SECRET"],
        &["GOOGLE_API_KEY", "GOOGLE_APP_ID"],
    ],
    maintenance: &[
        backend::maintenance::MAINTENANCE,
        backend::maintenance::TEAM,
    ],
    audit: Audit {
        areas: &[("documents.", Connections)],
        ..Audit::NONE
    },
    ..feature("documents")
};

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> dispatch_core::Typescript {
    dispatch_core::typescript!(
        cfg,
        DocumentsOverview,
        PickerSetup,
        PickerKeys,
        DriveFile,
        DocumentsConnection,
        ConnectionStatus,
        AccountKind,
        GoogleSignIn,
        ItemKind,
        NewKind,
        DocumentsItem,
        FolderStep,
        DocumentsFolder,
        SharingState,
        MySharing,
        DriveStorage,
        DocumentsAccount,
        GoogleConnected,
    )
}
