//! The platform owner's API, `/api/platform/*`, and its types.
pub mod types;
pub(crate) mod routes {
    pub mod audit;
    pub mod platform;
}

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        types::BetaMember,
        types::BrowserAdmission,
        types::BrowserHealth,
        types::DiagnosticsDsp,
        types::DiagnosticsRuntime,
        types::DspBetaTesters,
        types::DspFeatureReport,
        types::DspFeatures,
        types::DspVisibility,
        types::FeatureChange,
        types::FeatureState,
        types::MailHealth,
        types::PlatformDiagnostics,
        types::PlatformHealth,
        types::TransportHealth,
        types::Visibility,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    let mut all = routes::platform::routes();
    all.extend(routes::audit::routes());
    all
}
