//! The session's routes, and tenancy's and the audit log's types.
pub mod audit;
pub(crate) mod routes;
pub mod types;

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        audit::AuditArea,
        audit::AuditChange,
        audit::AuditCount,
        audit::AuditEvent,
        audit::AuditName,
        audit::AuditPage,
        audit::AuditReference,
        audit::AuditSubject,
        types::DspStatus,
        types::OwnerStatus,
        types::SiteDsp,
        types::SiteInfo,
        types::SiteKind,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    routes::routes()
}
