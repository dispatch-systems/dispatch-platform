//! The apps that may ask to connect. Four known apps by their published client documents
//! (CIMD), fetched from here and never from a browser; apps that register themselves
//! (RFC 7591), which may only send the owner back to an app on the owner's own computer; and,
//! once the owner lets them, websites and other apps, by a client document anywhere on the
//! public internet or by registering a website's redirect.
mod documents;
pub(super) mod redirects;

pub use documents::Documents;
pub use redirects::{allowed, destination, private_scheme};

use super::{Answer, Refusal};
use crate::GuardStore;
use crate::api::types::OAuthAppId;
use dispatch_core::{
    Result,
    db::{Store, at, iso, now},
    foundation::crypto,
};
use redirects::{registrable, registrable_website};
use serde_json::{Value, json};

/// An app as an authorization names it: its client id, the name it goes by, whether Dispatch
/// recognizes its published metadata or only has its word, and where it may be sent back to.
#[derive(Clone, Debug)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub known: bool,
    pub redirect_uris: Vec<String>,
}

struct KnownApp {
    app: OAuthAppId,
    url: &'static str,
    name: &'static str,
    copy: &'static str,
}

/// The apps Dispatch knows: their identity, each one's exact client document URL, the name
/// to fall back on, and the copy served instead of the network in fixture mode and tests.
const KNOWN: &[KnownApp] = &[
    KnownApp {
        app: OAuthAppId::Chatgpt,
        url: "https://chatgpt.com/oauth/client.json",
        name: "ChatGPT",
        copy: include_str!("clients/chatgpt.json"),
    },
    KnownApp {
        app: OAuthAppId::Codex,
        url: "https://chatgpt.com/oauth/codex/client.json",
        name: "Codex",
        copy: include_str!("clients/codex.json"),
    },
    KnownApp {
        app: OAuthAppId::ClaudeCode,
        url: "https://claude.ai/oauth/claude-code-client-metadata",
        name: "Claude Code",
        copy: include_str!("clients/claude-code.json"),
    },
    KnownApp {
        app: OAuthAppId::Hermes,
        url: "https://nousresearch.github.io/hermes-agent/docs/oauth/client-metadata.json",
        name: "Hermes Agent",
        copy: include_str!("clients/hermes.json"),
    },
];
/// What an app that registers itself may give: a few redirects, none of them long.
const MOST_REDIRECTS: usize = 5;
/// Registrations not yet given a token, across every address, before registering waits.
const MOST_UNUSED: i64 = 200;
/// An unused registration belongs only to the short pairing attempt that created it.
pub(super) const UNUSED_LIFETIME: i64 = 15 * 60 * 1000;

/// The known app whose client document is at `url`, as the owner's choice of apps names it.
pub fn known_app(url: &str) -> Option<OAuthAppId> {
    KNOWN
        .iter()
        .find(|known| known.url == url)
        .map(|known| known.app)
}

pub trait ClientStore {
    /// An app that registered itself, while it is still registered.
    fn registered_client(&self, id: &str) -> Result<Option<Client>>;

    /// Registers an app (RFC 7591): only a public client, whose every redirect opens an app on
    /// the owner's own computer, or is a website's while the owner lets websites and other
    /// apps connect. Answers the registration as the client is to keep it.
    fn register_oauth_client(&self, metadata: &Value) -> Result<Answer<Value>>;

    /// Notes that a registered app was given tokens, which keeps its registration.
    fn used_oauth_client(&self, id: &str) -> Result<()>;
}
impl ClientStore for Store {
    fn registered_client(&self, id: &str) -> Result<Option<Client>> {
        let Some(row) = self.platform.one(
            "SELECT name,redirect_uris FROM oauth_clients WHERE id=?",
            [id],
        )?
        else {
            return Ok(None);
        };
        Ok(Some(Client {
            id: id.to_owned(),
            name: dispatch_core::db::s(&row, "name").to_owned(),
            known: false,
            redirect_uris: serde_json::from_str(dispatch_core::db::s(&row, "redirect_uris"))?,
        }))
    }

    fn register_oauth_client(&self, metadata: &Value) -> Result<Answer<Value>> {
        let refused = |error, description: &str| Ok(Err(Refusal::new(error, description)));
        if self.oauth_pairing()?.open_until.is_none() {
            return Ok(Err(Refusal {
                error: "registration_closed",
                description: "Open Connect apps in Dispatch before registering an app".into(),
                status: 403,
            }));
        }
        let strings = |key: &str| -> Option<Vec<&str>> {
            metadata[key]
                .as_array()?
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<_>>>()
        };
        let within = |key: &str, allowed: &[&str]| {
            metadata.get(key).is_none()
                || strings(key).is_some_and(|values| values.iter().all(|v| allowed.contains(v)))
        };
        if !metadata.is_object() {
            return refused("invalid_client_metadata", "The body must be a JSON object");
        }
        if !matches!(
            metadata.get("token_endpoint_auth_method"),
            None | Some(Value::Null)
        ) && metadata["token_endpoint_auth_method"] != "none"
        {
            return refused(
                "invalid_client_metadata",
                "Only public clients may register: token_endpoint_auth_method must be none",
            );
        }
        if !within("grant_types", &["authorization_code", "refresh_token"])
            || !within("response_types", &["code"])
        {
            return refused(
                "invalid_client_metadata",
                "Only the authorization_code and refresh_token grants and the code response exist",
            );
        }
        let redirect_uris = strings("redirect_uris").unwrap_or_default();
        let local = self.oauth_app_allowed(OAuthAppId::Local)?;
        let websites = self.oauth_app_allowed(OAuthAppId::Web)?;
        if redirect_uris.is_empty()
            || redirect_uris.len() > MOST_REDIRECTS
            || !redirect_uris
                .iter()
                .all(|uri| (local && registrable(uri)) || (websites && registrable_website(uri)))
        {
            return refused(
                "invalid_redirect_uri",
                if local && websites {
                    "Up to 5 redirects, each a loopback http address, the app's own URI scheme \
                     or an https address"
                } else if local {
                    "Up to 5 redirects, each a loopback http address or the app's own URI scheme"
                } else if websites {
                    "Up to 5 redirects, each an https address"
                } else {
                    "The owner is not accepting dynamically registered apps"
                },
            );
        }
        self.platform.exec(
            "DELETE FROM oauth_clients WHERE last_used_at IS NULL AND created_at<?",
            [at(now() - UNUSED_LIFETIME)],
        )?;
        let unused = self.platform.count(
            "SELECT count(*) FROM oauth_clients WHERE last_used_at IS NULL",
            [],
        )?;
        if unused >= MOST_UNUSED {
            return Ok(Err(Refusal {
                error: "too_many_registrations",
                description: "Too many apps registered without connecting; try again later".into(),
                status: 429,
            }));
        }
        let name = metadata["client_name"]
            .as_str()
            .and_then(crate::names::shown)
            .unwrap_or_else(|| "Unnamed app".into());
        let id = crypto::id("dcr")?;
        let created = now();
        self.platform.exec(
            "INSERT INTO oauth_clients(id,name,redirect_uris,created_at) VALUES (?,?,?,?)",
            [
                id.as_str(),
                &name,
                &json!(redirect_uris).to_string(),
                &at(created),
            ],
        )?;
        Ok(Ok(json!({
            "client_id": id,
            "client_id_issued_at": created / 1000,
            "client_name": name,
            "redirect_uris": redirect_uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })))
    }

    fn used_oauth_client(&self, id: &str) -> Result<()> {
        self.platform.exec(
            "UPDATE oauth_clients SET last_used_at=? WHERE id=?",
            [iso(), id.to_owned()],
        )?;
        Ok(())
    }
}
