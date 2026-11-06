//! Slack's app configuration, Live/Fixture facade and shared API response policy.
mod oauth;
mod transfer;

use super::fixture;
use axum::body::Bytes;
use dispatch_core::{
    Error, Result,
    foundation::{config::Config, observability},
};
use futures_util::Stream;
use serde_json::{Value, json};
use std::{
    pin::Pin,
    sync::{LazyLock, OnceLock},
    time::{Duration, Instant},
};

const API: &str = "https://slack.com/api";

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("the TLS backend is built in")
});

/// A file's bytes as they arrive from Slack.
pub type Bytestream = Pin<Box<dyn Stream<Item = std::io::Result<Bytes>> + Send>>;

/// The platform's Slack app: its ID, the secret only the server holds, and the secret Slack
/// signs its requests with.
pub struct Client {
    id: String,
    secret: String,
    signing: String,
}
static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
/// This server's Slack app, read once from the settings Slack declares:
/// `DISPATCH_DEV_SLACK_CLIENT_ID`, `…_CLIENT_SECRET` and `…_SIGNING_SECRET` on Dev and its
/// previews, `DISPATCH_PRODUCTION_…` on Production. A server with only some won't start.
fn client(config: &Config) -> Option<&'static Client> {
    CLIENT
        .get_or_init(|| {
            Some(Client {
                id: config.setting("SLACK_CLIENT_ID")?,
                secret: config.setting("SLACK_CLIENT_SECRET")?,
                signing: config.setting("SLACK_SIGNING_SECRET")?,
            })
        })
        .as_ref()
}

/// A workspace's tokens for the bot: the access token its calls use, and, as Slack rotates
/// them, the refresh token that renews it and when it expires, in milliseconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub access: String,
    pub refresh: Option<String>,
    pub expires_at: Option<i64>,
}
/// What installing the app in a workspace gave Dispatch.
pub struct Installed {
    pub team: String,
    pub name: String,
    pub bot_user: String,
    pub app: String,
    pub scopes: Vec<String>,
    pub tokens: Tokens,
}

/// What Slack answered a call to one of its API methods: its answer, or to slow down, calling
/// again only after so long.
pub enum Called {
    Answered(Value),
    SlowDown(Duration),
}

/// Real Slack, or fixture mode's stand-in.
pub enum Slack {
    Live(&'static Client),
    Fixture,
}
impl Slack {
    /// The Slack this server talks to. Without the app's settings, there's none to install.
    pub fn of(config: &Config) -> Result<Self> {
        if config.fixture {
            return Ok(Self::Fixture);
        }
        client(config)
            .map(Self::Live)
            .ok_or_else(|| Error::new("slack_unavailable", 503))
    }
    pub fn available(config: &Config) -> bool {
        Self::of(config).is_ok()
    }
    /// The secret Slack signs each request it sends with.
    pub fn signing_secret(&self) -> &str {
        match self {
            Self::Live(client) => &client.signing,
            Self::Fixture => fixture::SIGNING_SECRET,
        }
    }
    /// Calls Slack's API method `method` as the bot, with `args`: any method, as a form,
    /// where a value that isn't text goes as JSON, as Slack takes a view or blocks. Slack's
    /// answer when it says `ok`, or how long Slack asks Dispatch to wait before calling again.
    pub async fn call(&self, token: &str, method: &str, args: &Value) -> Result<Called> {
        if let Self::Fixture = self {
            return fixture::call(token, method, args);
        }
        let fields: Vec<(String, String)> = args
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| {
                let text = match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (key.clone(), text)
            })
            .collect();
        let request = HTTP
            .post(format!("{API}/{method}"))
            .bearer_auth(token)
            .form(&fields);
        let response = timed(method, request).await.map_err(unreachable)?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let wait = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
                .unwrap_or(1);
            return Ok(Called::SlowDown(Duration::from_secs(wait)));
        }
        answer(method, response).await.map(Called::Answered)
    }
    /// Removes the app from the workspace whose token this is. Best effort: Dispatch forgets
    /// the workspace whatever Slack answers.
    pub async fn uninstall(&self, token: &str) {
        if let Self::Live(client) = self {
            let request = HTTP
                .post(format!("{API}/apps.uninstall"))
                .bearer_auth(token)
                .form(&[
                    ("client_id", client.id.as_str()),
                    ("client_secret", client.secret.as_str()),
                ]);
            if let Ok(response) = timed("apps.uninstall", request).await {
                let _ = answer("apps.uninstall", response).await;
            }
        }
    }
}

fn unreachable(_: reqwest::Error) -> Error {
    Error::new("slack_unreachable", 502)
}
/// Sends `request`, noting in the log which method it called and how long Slack took: never
/// what it sent, which holds tokens.
async fn timed(
    method: &str,
    request: reqwest::RequestBuilder,
) -> reqwest::Result<reqwest::Response> {
    let started = Instant::now();
    let answer = request.send().await;
    observability::event(
        "info",
        "slack.api",
        json!({
            "call": method,
            "elapsedMs": started.elapsed().as_millis() as u64,
            "status": answer.as_ref().ok().map(|response| response.status().as_u16()),
        }),
    );
    answer
}
/// Slack's answer, or what its refusal means. Slack answers 200 with `ok` false and its
/// reason in `error`.
async fn answer(method: &str, response: reqwest::Response) -> Result<Value> {
    if response.status().is_server_error() {
        return Err(Error::new("slack_unreachable", 502));
    }
    let value: Value = response.json().await.map_err(unreachable)?;
    if value["ok"] == true {
        return Ok(value);
    }
    let reason = value["error"].as_str().unwrap_or("");
    observability::event(
        "warn",
        "slack.refused",
        json!({"call": method, "reason": reason}),
    );
    Err(refusal(reason))
}
/// What a refusal means: Slack no longer honours the workspace's tokens, so only reconnecting
/// helps; its token expired, so a new one does; the bot lacks a permission the workspace hasn't
/// approved yet, so updating does; Slack is busy; or the request itself failed.
fn refusal(reason: &str) -> Error {
    match reason {
        "invalid_auth"
        | "not_authed"
        | "account_inactive"
        | "token_revoked"
        | "invalid_refresh_token"
        | "app_uninstalled"
        | "team_access_not_granted"
        | "invalid_code"
        | "code_already_used" => Error::new("slack_connection_broken", 409),
        "token_expired" => Error::new("slack_token_expired", 409),
        "missing_scope" => Error::new("slack_needs_update", 409),
        "ratelimited" => Error::new("slack_rate_limited", 503),
        "file_not_found" | "file_deleted" => Error::new("slack_file_gone", 404),
        _ => Error::new("slack_refused", 502),
    }
}
