//! Backups' API contracts: a DSP's Backups tab, which only the platform owner opens.
pub mod routes;
pub mod types;

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        types::Backup,
        types::BackupItem,
        types::BackupRule,
        types::BackupRuns,
        types::BackupStatus,
        types::DspBackups,
        types::KeepUnit,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    routes::routes()
}
