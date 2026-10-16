//! Slack, as Dispatch talks to it: installing the app in a workspace, and calling Slack's API
//! there as the app's bot. Fixture mode's Slack answers in-process instead.
use super::{
    app::{RETURN_PATH, SCOPES},
    fixture,
};
use dispatch_core::{
    Error, Result,
    db::now,
    foundation::{config::Config, observability},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{LazyLock, OnceLock},
    time::{Duration, Instant},
};

const AUTHORIZE: &str = "https://slack.com/oauth/v2/authorize";
const API: &str = "https://slack.com/api";

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("the TLS backend is built in")
});

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
    /// Where the browser goes to install the app. `team` skips Slack's choice of workspace, as
    /// reconnecting or updating one does. Fixture mode skips Slack: the browser comes straight
    /// back with a code naming the workspace, `fixture:<team>`.
    pub fn install_url(&self, origin: &str, state: &str, team: Option<&str>) -> String {
        let back = format!("{origin}{RETURN_PATH}");
        let (base, query) = match self {
            Self::Fixture => (
                back.as_str(),
                vec![
                    ("state", state.to_owned()),
                    ("code", format!("fixture:{}", team.unwrap_or_default())),
                ],
            ),
            Self::Live(client) => (
                AUTHORIZE,
                [
                    ("client_id", client.id.clone()),
                    ("scope", SCOPES.join(",")),
                    ("redirect_uri", back.clone()),
                    ("state", state.to_owned()),
                ]
                .into_iter()
                .chain(team.map(|team| ("team", team.to_owned())))
                .collect(),
            ),
        };
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        format!("{base}?{query}")
    }
    /// Trades the code Slack sent the browser back with for the workspace's tokens.
    pub async fn exchange(&self, origin: &str, code: &str) -> Result<Installed> {
        let client = match self {
            Self::Fixture => return fixture::install(code),
            Self::Live(client) => client,
        };
        let back = format!("{origin}{RETURN_PATH}");
        let granted: Granted = form(
            "oauth.v2.access",
            &[
                ("client_id", client.id.as_str()),
                ("client_secret", client.secret.as_str()),
                ("code", code),
                ("redirect_uri", back.as_str()),
            ],
        )
        .await
        .map_err(|error| match error.code.as_str() {
            "slack_connection_broken" => Error::new("slack_connect_expired", 409),
            _ => error,
        })?;
        // Installing across a whole Enterprise Grid organization, which the app doesn't offer.
        if granted.is_enterprise_install {
            self.uninstall(&granted.access_token).await;
            return Err(Error::new("slack_organization_install", 400));
        }
        let team = granted
            .team
            .ok_or_else(|| Error::new("slack_install_failed", 502))?;
        Ok(Installed {
            team: team.id,
            name: team.name.unwrap_or_default(),
            bot_user: granted.bot_user_id.unwrap_or_default(),
            app: granted.app_id.unwrap_or_default(),
            scopes: granted
                .scope
                .split(',')
                .filter(|scope| !scope.is_empty())
                .map(str::to_owned)
                .collect(),
            tokens: Tokens {
                access: granted.access_token,
                refresh: granted.refresh_token,
                expires_at: granted.expires_in.map(|secs| now() + secs * 1000),
            },
        })
    }
    /// New tokens for the workspace, from its refresh token, which Slack honours only once.
    /// One Slack refuses means Dispatch was removed there, or its tokens revoked.
    pub async fn refresh(&self, refresh: &str) -> Result<Tokens> {
        let client = match self {
            Self::Fixture => return fixture::refresh(refresh),
            Self::Live(client) => client,
        };
        let granted: Granted = form(
            "oauth.v2.access",
            &[
                ("client_id", client.id.as_str()),
                ("client_secret", client.secret.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
            ],
        )
        .await?;
        Ok(Tokens {
            access: granted.access_token,
            refresh: granted.refresh_token,
            expires_at: granted.expires_in.map(|secs| now() + secs * 1000),
        })
    }
    /// Calls Slack's API method `method` as the bot, with `args`: any method, as a form,
    /// where a value that isn't text goes as JSON, as Slack takes a view or blocks. Slack's
    /// answer when it says `ok`. When Slack asks Dispatch to slow down, it waits as long as
    /// Slack says, up to half a minute, and tries again, twice at most.
    pub async fn call(&self, token: &str, method: &str, args: &Value) -> Result<Value> {
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
        let url = format!("{API}/{method}");
        for attempt in 0.. {
            let request = HTTP.post(&url).bearer_auth(token).form(&fields);
            let response = timed(method, request).await.map_err(unreachable)?;
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
                    .unwrap_or(1);
                if attempt >= 2 || wait > 30 {
                    return Err(Error::new("slack_rate_limited", 503));
                }
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            return answer(method, response).await;
        }
        unreachable!("the loop returns")
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

/// What `oauth.v2.access` answers, on installing and on renewing tokens.
#[derive(Deserialize)]
struct Granted {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[serde(default)]
    scope: String,
    bot_user_id: Option<String>,
    app_id: Option<String>,
    team: Option<Team>,
    #[serde(default)]
    is_enterprise_install: bool,
}
#[derive(Deserialize)]
struct Team {
    id: String,
    name: Option<String>,
}

fn unreachable(_: reqwest::Error) -> Error {
    Error::new("slack_unreachable", 502)
}
/// Calls an `oauth.v2` method, which takes the app's own credentials in a form, no token.
async fn form<T: for<'de> Deserialize<'de>>(method: &str, fields: &[(&str, &str)]) -> Result<T> {
    let request = HTTP.post(format!("{API}/{method}")).form(fields);
    let response = timed(method, request).await.map_err(unreachable)?;
    serde_json::from_value(answer(method, response).await?)
        .map_err(|_| Error::new("slack_install_failed", 502))
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
        _ => Error::new("slack_refused", 502),
    }
}
