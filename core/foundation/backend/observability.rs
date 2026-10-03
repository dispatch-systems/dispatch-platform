//! Structured events contain only selected operational fields, never raw requests.
use serde_json::{Value, json};

#[derive(Default)]
pub struct RequestContext {
    pub actor: Option<String>,
    pub tenant: Option<String>,
    pub account: Option<String>,
    pub route: Option<&'static str>,
    pub bulk: bool,
    /// An agent's call, as the request notes it for the Agents page's Activity log.
    pub agent: crate::mcp::activity::Noted,
}
pub type RequestTrace = std::sync::Arc<std::sync::Mutex<RequestContext>>;

pub fn event(level: &str, event: &str, fields: Value) {
    eprintln!(
        "{}",
        json!({"at":crate::db::iso(),"level":level,"event":event,"fields":fields})
    );
}

/// What the request log names a request by until its route reads it, for core's own paths
/// and otherwise by the part of the API it is in. A feature's route names its own
/// (`Route::logged`).
pub fn route(path: &str) -> &'static str {
    match path {
        "/api/health" => "/api/health",
        "/api/auth/login" => "/api/auth/login",
        "/api/auth/logout" => "/api/auth/logout",
        "/api/auth/password" => "/api/auth/password",
        "/api/auth/forgot-password" => "/api/auth/forgot-password",
        "/api/auth/reset-password" => "/api/auth/reset-password",
        "/api/session" => "/api/session",
        "/api/session/dsp" => "/api/session/dsp",
        "/api/dsp/collection-updates" => "/api/dsp/collection-updates",
        "/api/platform/health" => "/api/platform/health",
        _ if path.starts_with("/api/invitations/") => "/api/invitations/:token/*",
        _ if path.starts_with("/api/dsp/") => "/api/dsp/*",
        _ if path.starts_with("/api/platform/") => "/api/platform/*",
        "/" => "/",
        _ if path.starts_with("/assets/") => "/assets/*",
        _ if path.starts_with("/.well-known/") => "/.well-known/*",
        _ if path.starts_with("/oauth/") => "/oauth/*",
        _ => "/unmatched",
    }
}
