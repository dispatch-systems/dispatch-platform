//! Accounts' API: sign-in and security, and what they accept and answer with.
pub mod requests;
pub mod types;
pub(crate) mod routes {
    pub mod invitations;
    pub mod security;
    pub mod sign_in;
}

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        types::AccountSession,
        types::AuthenticatorSetup,
        types::Dsp,
        types::DspProfile,
        types::DspSummary,
        types::DspView,
        types::MailMessage,
        types::Member,
        types::PasskeySummary,
        types::PublicUser,
        types::Role,
        types::RoleSummary,
        types::RuntimeSource,
        types::SecurityStatus,
        types::SessionResponse,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    let mut all = routes::sign_in::routes();
    all.extend(routes::security::routes());
    all.extend(routes::invitations::routes());
    all
}
