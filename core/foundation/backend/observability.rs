//! Structured events contain only selected operational fields, never raw requests.
use serde_json::{Value, json};

#[derive(Default)]
pub struct RequestContext {
    pub request_id: Option<String>,
    pub actor: Option<String>,
    pub tenant: Option<String>,
    pub account: Option<String>,
    pub route: Option<&'static str>,
    pub bulk: bool,
    /// What an agent's request notes of its call as it goes, in the agents' piece's own
    /// terms, handed back to it once the request is answered.
    pub agent: Option<Box<dyn std::any::Any + Send>>,
}
pub type RequestTrace = std::sync::Arc<std::sync::Mutex<RequestContext>>;

tokio::task_local! {
    /// Each HTTP future's trace. Blocking work takes its own clone before spawning;
    /// background jobs outside this scope have no request identity.
    pub(crate) static REQUEST_TRACE: RequestTrace;
}

std::thread_local! {
    /// The events this thread logged while `recorded` runs.
    static RECORDED: std::cell::RefCell<Option<Vec<Value>>> = const { std::cell::RefCell::new(None) };
}

pub fn event(level: &str, event: &str, fields: Value) {
    let line = json!({"at":crate::db::iso(),"level":level,"event":event,"fields":fields});
    RECORDED.with_borrow_mut(|recorded| {
        if let Some(recorded) = recorded {
            recorded.push(line.clone());
        }
    });
    eprintln!("{line}");
}

/// Runs `work`, answering what it answers and each event it logged on this thread, as logged:
/// what a command reports of its own work, such as an error it logged and went on from.
pub fn recorded<T>(work: impl FnOnce() -> T) -> (T, Vec<Value>) {
    let outer = RECORDED.replace(Some(Vec::new()));
    let answer = work();
    let events = RECORDED.replace(outer).unwrap_or_default();
    (answer, events)
}

/// What the request log names a request by until its route reads it, for core's own paths
/// and otherwise by the part of the API it is in. A feature's route names its own
/// (`Route::logged`).
pub fn route(path: &str) -> &'static str {
    match path {
        "/api/health" => "/api/health",
        "/api/auth/login" => "/api/auth/login",
        "/api/auth/observation" => "/api/auth/observation",
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
