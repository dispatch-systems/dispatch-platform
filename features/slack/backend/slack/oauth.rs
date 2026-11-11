//! Installing the app and renewing the workspace's tokens.
use super::{API, HTTP, Installed, Slack, Tokens, answer, timed, unreachable};
use crate::backend::{app::RETURN_PATH, fixture};
use dispatch_core::{Error, Result, db::now};
use serde::Deserialize;

const AUTHORIZE: &str = "https://slack.com/oauth/v2/authorize";

impl Slack {
    /// Where the browser goes to install the app. `team` skips Slack's choice of workspace, as
    /// reconnecting or updating one does. Fixture mode skips Slack: the browser comes straight
    /// back with a code naming the workspace, `fixture:<team>`.
    pub fn install_url(
        &self,
        origin: &str,
        state: &str,
        team: Option<&str>,
        scopes: &[&str],
    ) -> String {
        let back = format!("{origin}{RETURN_PATH}");
        let (base, query) = match self {
            Self::Fixture => (
                back.as_str(),
                vec![
                    ("state", state.to_owned()),
                    (
                        "code",
                        format!("fixture:{}:{}", team.unwrap_or_default(), scopes.join(",")),
                    ),
                ],
            ),
            Self::Live(client) => (
                AUTHORIZE,
                [
                    ("client_id", client.id.clone()),
                    ("scope", scopes.join(",")),
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

/// Calls an `oauth.v2` method, which takes the app's own credentials in a form, no token.
async fn form<T: for<'de> Deserialize<'de>>(method: &str, fields: &[(&str, &str)]) -> Result<T> {
    let request = HTTP.post(format!("{API}/{method}")).form(fields);
    let response = timed(method, request).await.map_err(unreachable)?;
    serde_json::from_value(answer(method, response).await?)
        .map_err(|_| Error::new("slack_install_failed", 502))
}

#[cfg(test)]
#[path = "../../tests/backend/slack/oauth.rs"]
mod tests;
