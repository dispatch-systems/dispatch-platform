//! Settings' API: every section a member may see of a DSP's Settings, and saving one, with
//! the types they answer with.
pub(crate) mod routes;
pub mod types;

/// Its API types, which the app's export writes to TypeScript in its `api/generated/`.
#[cfg(feature = "ts")]
pub fn typescript(cfg: &ts_rs::Config) -> crate::Typescript {
    crate::typescript!(
        cfg,
        types::SettingsAnswer,
        types::SettingsField,
        types::SettingsGroup,
        types::SettingsKind,
        types::SettingsOption,
        types::SettingsPage,
        types::SettingsPreset,
        types::SettingsSave,
        types::SettingsSaved,
        types::SettingsScheduleChoice,
        types::SettingsScheduleOption,
        types::SettingsScope,
        types::SettingsSection,
        types::SettingsStart,
    )
}

/// Its routes, each with its path and access, which `tests/api/routes.txt` lists.
pub fn routes() -> Vec<crate::server::http::Route> {
    routes::routes()
}
