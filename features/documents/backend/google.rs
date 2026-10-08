//! Google, as Documents talks to it: signing a DSP's account in, and the Drive calls it makes
//! with that account. Fixture mode answers in-process, so previews and tests never reach Google.
use dispatch_core::{
    Error, Result,
    foundation::{config::Config, crypto, observability},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    sync::{LazyLock, OnceLock},
    time::Duration,
};

const AUTHORIZE: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const REVOKE: &str = "https://oauth2.googleapis.com/revoke";
const USERINFO: &str = "https://openidconnect.googleapis.com/v1/userinfo";
pub(super) const FILES: &str = "https://www.googleapis.com/drive/v3/files";
/// Only the files Dispatch makes or is given: Google asks no review for it.
const DRIVE_FILE: &str = "https://www.googleapis.com/auth/drive.file";
const SCOPES: &str = "openid email https://www.googleapis.com/auth/drive.file";
/// A member linking their own Google account: only which address it is.
const LINK_SCOPES: &str = "openid email";
pub(super) const FOLDER: &str = "application/vnd.google-apps.folder";
/// Where Google sends the browser back, on this server's own origin.
pub const RETURN_PATH: &str = "/api/documents/google/return";
/// The account a preview's fixture Google signs in as.
const FIXTURE_ACCOUNT: &str = "documents@example.com";
/// The account a member links in a preview, as fixture Google signs them in.
const FIXTURE_MEMBER: &str = "teammate@example.com";

pub(super) static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("the TLS backend is built in")
});

/// The Google account a sign-in proved, and whether a Google Workspace holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    pub workspace: bool,
}
/// What signing in gave Dispatch: the account, a refresh token to keep, and an access token
/// for the calls right after.
pub struct Granted {
    pub account: Account,
    pub refresh: String,
    pub access: String,
}
/// An access token for the account's Drive, and how long Google honors it.
pub struct Access {
    pub token: String,
    pub lasts: Duration,
}

/// The platform's Google sign-in client: its ID, and the secret only the server holds.
pub struct Client {
    id: String,
    secret: String,
}
static CLIENT: OnceLock<Option<Client>> = OnceLock::new();
/// This server's Google sign-in client, read once from its own settings, as the platform's
/// mail settings are: `DISPATCH_DEV_GOOGLE_CLIENT_ID` and `…_SECRET` on Dev and its previews,
/// `DISPATCH_PRODUCTION_…` on Production. Half of one is none, and is logged.
fn client(config: &Config) -> Option<&'static Client> {
    CLIENT
        .get_or_init(|| {
            let prefix = if config.environment == "preview" {
                "DISPATCH_DEV"
            } else {
                "DISPATCH_PRODUCTION"
            };
            let setting = |name: &str| {
                std::env::var(format!("{prefix}_{name}"))
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            };
            match (setting("GOOGLE_CLIENT_ID"), setting("GOOGLE_CLIENT_SECRET")) {
                (Some(id), Some(secret)) => Some(Client { id, secret }),
                (None, None) => None,
                _ => {
                    observability::event("error", "documents_google_half_configured", json!({}));
                    None
                }
            }
        })
        .as_ref()
}

/// Real Google, or fixture mode's stand-in.
pub enum Google {
    Live(&'static Client),
    Fixture,
}
impl Google {
    /// The Google this server talks to. Without a sign-in client, there's none to connect.
    pub fn of(config: &Config) -> Result<Self> {
        if config.fixture {
            return Ok(Self::Fixture);
        }
        client(config)
            .map(Self::Live)
            .ok_or_else(|| Error::new("google_unavailable", 503))
    }
    pub fn available(config: &Config) -> bool {
        Self::of(config).is_ok()
    }
    /// Where the browser goes to sign in. `hint` picks the account when reconnecting.
    pub fn sign_in_url(
        &self,
        origin: &str,
        state: &str,
        verifier: &str,
        hint: Option<&str>,
    ) -> String {
        let back = format!("{origin}{RETURN_PATH}");
        let (base, query) = match self {
            // Signing in at the fixture skips Google: the browser comes straight back.
            Self::Fixture => (
                back.as_str(),
                vec![
                    ("state", state.to_owned()),
                    (
                        "code",
                        format!("fixture:{}", hint.unwrap_or(FIXTURE_ACCOUNT)),
                    ),
                ],
            ),
            Self::Live(client) => (
                AUTHORIZE,
                [
                    ("client_id", client.id.clone()),
                    ("redirect_uri", back.clone()),
                    ("response_type", "code".into()),
                    ("scope", SCOPES.into()),
                    // A refresh token on every sign-in, so a reconnect replaces a revoked one.
                    ("access_type", "offline".into()),
                    ("prompt", "consent".into()),
                    ("state", state.to_owned()),
                    ("code_challenge", crypto::s256(verifier)),
                    ("code_challenge_method", "S256".into()),
                ]
                .into_iter()
                .chain(hint.map(|email| ("login_hint", email.to_owned())))
                .collect(),
            ),
        };
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        format!("{base}?{query}")
    }
    /// Where a member goes to link their own Google account: Google asks them which, and for
    /// their address only.
    pub fn link_url(&self, origin: &str, state: &str, verifier: &str) -> String {
        let back = format!("{origin}{RETURN_PATH}");
        let (base, query) = match self {
            Self::Fixture => (
                back.as_str(),
                vec![
                    ("state", state.to_owned()),
                    ("code", format!("fixture:{FIXTURE_MEMBER}")),
                ],
            ),
            Self::Live(client) => (
                AUTHORIZE,
                vec![
                    ("client_id", client.id.clone()),
                    ("redirect_uri", back.clone()),
                    ("response_type", "code".into()),
                    ("scope", LINK_SCOPES.into()),
                    ("prompt", "select_account".into()),
                    ("state", state.to_owned()),
                    ("code_challenge", crypto::s256(verifier)),
                    ("code_challenge_method", "S256".into()),
                ],
            ),
        };
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        format!("{base}?{query}")
    }
    /// The Google address a member's sign-in proved, for linking it. Dispatch keeps no token
    /// of it: the one Google gave is given back at once.
    pub async fn linked_email(&self, origin: &str, code: &str, verifier: &str) -> Result<String> {
        let client = match self {
            Self::Fixture => return fixture_grant(code).map(|granted| granted.account.email),
            Self::Live(client) => client,
        };
        let back = format!("{origin}{RETURN_PATH}");
        let tokens: Tokens = form(
            TOKEN,
            &[
                ("client_id", client.id.as_str()),
                ("client_secret", client.secret.as_str()),
                ("code", code),
                ("code_verifier", verifier),
                ("grant_type", "authorization_code"),
                ("redirect_uri", back.as_str()),
            ],
        )
        .await
        .map_err(|error| match error.code.as_str() {
            "google_connection_broken" => Error::new("documents_connect_expired", 409),
            _ => error,
        })?;
        let profile: Result<Profile> =
            send(HTTP.get(USERINFO).bearer_auth(&tokens.access_token)).await;
        let _ = HTTP
            .post(REVOKE)
            .form(&[("token", tokens.access_token.as_str())])
            .send()
            .await;
        let profile = profile?;
        profile
            .email
            .filter(|_| profile.email_verified)
            .map(|email| email.to_lowercase())
            .ok_or_else(|| Error::new("google_email_unverified", 400))
    }
    /// Trades the code Google sent the browser back with for the account's tokens.
    pub async fn exchange(&self, origin: &str, code: &str, verifier: &str) -> Result<Granted> {
        let client = match self {
            Self::Fixture => return fixture_grant(code),
            Self::Live(client) => client,
        };
        let back = format!("{origin}{RETURN_PATH}");
        // A code Google no longer honors was used already, or waited too long.
        let tokens: Tokens = form(
            TOKEN,
            &[
                ("client_id", client.id.as_str()),
                ("client_secret", client.secret.as_str()),
                ("code", code),
                ("code_verifier", verifier),
                ("grant_type", "authorization_code"),
                ("redirect_uri", back.as_str()),
            ],
        )
        .await
        .map_err(|error| match error.code.as_str() {
            "google_connection_broken" => Error::new("documents_connect_expired", 409),
            _ => error,
        })?;
        let access = tokens.access_token;
        let Some(refresh) = tokens.refresh_token else {
            return Err(Error::new("google_sign_in_failed", 502));
        };
        // Google lets the person untick Drive on its consent screen.
        if !tokens.scope.split(' ').any(|scope| scope == DRIVE_FILE) {
            self.revoke(&refresh).await;
            return Err(Error::new("google_drive_not_allowed", 400));
        }
        let profile: Profile = send(HTTP.get(USERINFO).bearer_auth(&access)).await?;
        let Some(email) = profile.email.filter(|_| profile.email_verified) else {
            self.revoke(&refresh).await;
            return Err(Error::new("google_email_unverified", 400));
        };
        Ok(Granted {
            account: Account {
                email: email.to_lowercase(),
                workspace: profile.hd.is_some(),
            },
            refresh,
            access,
        })
    }
    /// A fresh access token for the account. One Google won't renew means the owner removed
    /// Dispatch, or the account is gone: the connection is broken until someone reconnects.
    pub async fn access(&self, refresh: &str) -> Result<Access> {
        let client = match self {
            Self::Fixture => return fixture_access(refresh),
            Self::Live(client) => client,
        };
        let tokens: Tokens = form(
            TOKEN,
            &[
                ("client_id", client.id.as_str()),
                ("client_secret", client.secret.as_str()),
                ("refresh_token", refresh),
                ("grant_type", "refresh_token"),
            ],
        )
        .await?;
        Ok(Access {
            token: tokens.access_token,
            lasts: Duration::from_secs(tokens.expires_in.unwrap_or(3600)),
        })
    }
    /// Makes a folder at the top of the account's Drive and answers its ID.
    pub async fn create_folder(&self, access: &str, name: &str) -> Result<String> {
        Ok(self.create(access, name, FOLDER, None).await?.id)
    }
    /// Whether the folder is still there and out of the trash.
    pub async fn folder_usable(&self, access: &str, id: &str) -> Result<bool> {
        if let Self::Fixture = self {
            return super::drive::fixture_has(access, id);
        }
        let response = HTTP
            .get(format!("{FILES}/{id}"))
            .bearer_auth(access)
            .query(&[("fields", "id,trashed")])
            .send()
            .await
            .map_err(unreachable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        let file: Folder = read(response).await?;
        Ok(!file.trashed)
    }
    /// Gives the refresh token back, so the account no longer lists Dispatch. Best effort:
    /// Dispatch forgets the token whatever Google answers.
    pub async fn revoke(&self, refresh: &str) {
        if let Self::Live(_) = self {
            let _ = HTTP.post(REVOKE).form(&[("token", refresh)]).send().await;
        }
    }
}

/// The folder's address in Google Drive.
pub fn folder_url(id: &str) -> String {
    format!("https://drive.google.com/drive/folders/{id}")
}

/// Fixture mode's code names the account it signs in as, `fixture:<email>`: anyone's own
/// Google account, never a Workspace's. Its tokens name the account too, as Google's stand for
/// it: the access token is `fixture:<email>`, which picks the account's Drive.
fn fixture_grant(code: &str) -> Result<Granted> {
    let email = code
        .strip_prefix("fixture:")
        .filter(|email| email.contains('@') && !email.contains(':'))
        .ok_or_else(|| Error::new("google_sign_in_failed", 502))?
        .to_lowercase();
    Ok(Granted {
        account: Account {
            email: email.clone(),
            workspace: false,
        },
        refresh: format!("fixture:{}:{email}", crypto::token()?),
        access: format!("fixture:{email}"),
    })
}
fn fixture_access(refresh: &str) -> Result<Access> {
    let email = refresh
        .splitn(3, ':')
        .nth(2)
        .filter(|_| refresh.starts_with("fixture:"))
        .ok_or_else(|| Error::new("google_connection_broken", 409))?;
    Ok(Access {
        token: format!("fixture:{email}"),
        lasts: Duration::from_secs(3600),
    })
}

#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(default)]
    scope: String,
    expires_in: Option<u64>,
}
#[derive(Deserialize)]
struct Profile {
    email: Option<String>,
    #[serde(default)]
    email_verified: bool,
    /// The Google Workspace domain, for a Workspace account only.
    hd: Option<String>,
}
#[derive(Deserialize)]
struct Folder {
    #[serde(default)]
    trashed: bool,
}
/// Google's refusal: the token endpoint names its error, Drive names the reason of its first.
#[derive(Deserialize)]
struct Refusal {
    error: Option<serde_json::Value>,
}
impl Refusal {
    fn reason(&self) -> Option<&str> {
        let error = self.error.as_ref()?;
        error
            .as_str()
            .or_else(|| error.pointer("/errors/0/reason")?.as_str())
    }
}

pub(super) fn unreachable(_: reqwest::Error) -> Error {
    Error::new("google_unreachable", 502)
}
async fn form<T: for<'de> Deserialize<'de>>(url: &str, fields: &[(&str, &str)]) -> Result<T> {
    send(HTTP.post(url).form(fields)).await
}
pub(super) async fn send<T: for<'de> Deserialize<'de>>(
    request: reqwest::RequestBuilder,
) -> Result<T> {
    read(request.send().await.map_err(unreachable)?).await
}
/// Google's answer, or what its refusal means for the connection.
pub(super) async fn read<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let body = response.bytes().await.map_err(unreachable)?;
    if status.is_success() {
        return serde_json::from_slice(&body).map_err(|_| Error::new("google_sign_in_failed", 502));
    }
    let refused = serde_json::from_slice::<Refusal>(&body).ok();
    Err(refusal(
        status.as_u16(),
        refused.as_ref().and_then(Refusal::reason),
    ))
}
/// What a refusal means: Google no longer honors the account's token, so only a reconnect
/// helps; the account's storage is full; the file is gone or was never Dispatch's to reach;
/// Google is busy or down, so trying later does; or the request itself failed.
fn refusal(status: u16, error: Option<&str>) -> Error {
    match (status, error) {
        // The token endpoint's word for a refresh token or code it no longer honors.
        (_, Some("invalid_grant")) | (401, _) => Error::new("google_connection_broken", 409),
        (_, Some("storageQuotaExceeded")) => Error::new("documents_storage_full", 409),
        (_, Some("exportSizeLimitExceeded")) => Error::new("documents_export_too_large", 409),
        (404, _) | (403, Some("insufficientFilePermissions" | "appNotAuthorizedToFile")) => {
            Error::new("documents_item_not_found", 404)
        }
        (403 | 429, _) | (500.., _) => Error::new("google_unreachable", 502),
        _ => Error::new("google_sign_in_failed", 502),
    }
}

#[cfg(test)]
#[path = "../tests/backend/google.rs"]
mod tests;
