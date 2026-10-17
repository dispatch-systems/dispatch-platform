//! AI Support's session, owner and dedicated worker API contracts.
pub mod routes;
pub mod types;

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        types::SupportAttachment,
        types::SupportDetail,
        types::SupportEvidence,
        types::SupportIntake,
        types::SupportIntakeOutcome,
        types::SupportMessage,
        types::SupportNotification,
        types::SupportProposal,
        types::SupportTicket,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    routes::routes()
}
