//! The apps that may ask to connect. Four known apps by their published client documents
//! (CIMD), fetched from here and never from a browser; apps that register themselves
//! (RFC 7591), which may only send the owner back to an app on the owner's own computer; and,
//! once the owner lets them, websites and other apps, by a client document anywhere on the
//! public internet or by registering a website's redirect.
use super::{
    Answer, Refusal,
    network::{self, Network},
};
use crate::GuardStore;
use crate::api::types::OAuthAppId;
use dispatch_core::{
    Error, Result,
    db::{Store, at, iso, now},
    ensure,
    foundation::{config::Config, crypto, observability},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

/// An app as an authorization names it: its client id, the name it goes by, whether Dispatch
/// recognizes its published metadata or only has its word, and where it may be sent back to.
#[derive(Clone, Debug)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub known: bool,
    pub redirect_uris: Vec<String>,
}

/// The apps Dispatch knows: the exact URL of each one's client document, the name to fall
/// back on, and the copy served instead of the network in fixture mode and tests.
const KNOWN: &[(&str, &str, &str)] = &[
    (
        "https://chatgpt.com/oauth/client.json",
        "ChatGPT",
        include_str!("clients/chatgpt.json"),
    ),
    (
        "https://chatgpt.com/oauth/codex/client.json",
        "Codex",
        include_str!("clients/codex.json"),
    ),
    (
        "https://claude.ai/oauth/claude-code-client-metadata",
        "Claude Code",
        include_str!("clients/claude-code.json"),
    ),
    (
        "https://nousresearch.github.io/hermes-agent/docs/oauth/client-metadata.json",
        "Hermes Agent",
        include_str!("clients/hermes.json"),
    ),
];
/// Which kind of app each known app is, in the same order, for the owner's choice of apps.
const KNOWN_APPS: [OAuthAppId; KNOWN.len()] = [
    OAuthAppId::Chatgpt,
    OAuthAppId::Codex,
    OAuthAppId::ClaudeCode,
    OAuthAppId::Hermes,
];
/// A document is fetched again after an hour, and while that fails the last one serves a day.
/// A failed fetch is not tried again for a minute.
const FRESH: i64 = 60 * 60 * 1000;
const KEPT: i64 = 24 * 60 * 60 * 1000;
const RETRY: i64 = 60 * 1000;
const LARGEST: usize = 64 * 1024;
/// What an app that registers itself may give: a few redirects, none of them long.
const MOST_REDIRECTS: usize = 5;
const LONGEST_REDIRECT: usize = 512;
/// Registrations not yet given a token, across every address, before registering waits.
const MOST_UNUSED: i64 = 200;
/// An unused registration belongs only to the short pairing attempt that created it.
pub(super) const UNUSED_LIFETIME: i64 = 15 * 60 * 1000;
/// Websites' documents kept at once, and fetched at once.
const MOST_WEBSITES: usize = 256;
/// Every request waiting for or performing a website-document fetch. Active network work is
/// bounded more tightly below; this cap also bounds per-URL mutex and semaphore waiters.
const WEBSITE_REQUESTS: usize = 32;
const WEBSITE_FETCHES: usize = 4;
/// Native apps known by a scheme without a dot, which RFC 8252 §7.1 would otherwise ask for.
const NATIVE: &[&str] = &[
    "cursor",
    "vscode",
    "vscode-insiders",
    "vscodium",
    "windsurf",
];
/// Schemes that are no app's own: the web's, and those a browser would run or read itself.
const NOT_PRIVATE: &[&str] = &[
    "http",
    "https",
    "javascript",
    "data",
    "file",
    "vbscript",
    "about",
    "blob",
    "ftp",
    "ws",
    "wss",
    "mailto",
];
const LOOPBACK: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

type Found = std::result::Result<Client, &'static str>;

/// The known apps' documents as last fetched, and when a fetch last failed. One fetch per
/// app runs at a time; the requests that wait for it take its answer. Websites' documents
/// are kept the same way, by URL, a few hundred at most.
pub struct Documents {
    fetched: Mutex<HashMap<&'static str, Fetched>>,
    fetching: [tokio::sync::Mutex<()>; KNOWN.len()],
    websites: Mutex<HashMap<String, Website>>,
    website_requests: tokio::sync::Semaphore,
    website_fetches: tokio::sync::Semaphore,
    /// Where websites' documents come from, when not the internet or fixture mode's.
    network: Mutex<Option<Arc<dyn Network>>>,
}
impl Default for Documents {
    fn default() -> Self {
        Self {
            fetched: Mutex::default(),
            fetching: Default::default(),
            websites: Mutex::default(),
            website_requests: tokio::sync::Semaphore::new(WEBSITE_REQUESTS),
            website_fetches: tokio::sync::Semaphore::new(WEBSITE_FETCHES),
            network: Mutex::default(),
        }
    }
}
/// A website's document as last fetched, the fetch under way for it, and when it was last
/// asked for, so the longest unused goes first when too many are kept.
#[derive(Default)]
struct Website {
    fetched: Fetched,
    fetching: Arc<tokio::sync::Mutex<()>>,
    asked: i64,
}
#[derive(Clone, Default)]
struct Fetched {
    document: Option<(i64, Client)>,
    failed: Option<i64>,
}
impl Fetched {
    /// What an app's document answers without fetching it, when that is settled: a fresh
    /// document, or after a recent failure the last document while it is kept.
    fn settled(&self) -> Option<Found> {
        if let Some((at, client)) = &self.document
            && now() - at < FRESH
        {
            return Some(Ok(client.clone()));
        }
        self.failed
            .is_some_and(|at| now() - at < RETRY)
            .then(|| self.kept())
    }
    fn kept(&self) -> Found {
        self.document
            .as_ref()
            .filter(|(at, _)| now() - at < KEPT)
            .map(|(_, client)| client.clone())
            .ok_or("app_unavailable")
    }
}
impl Documents {
    /// The known app whose client document is at `url`, or the error page's code:
    /// `unknown_app` for any other URL, `app_unavailable` when its document cannot be had.
    pub async fn client(&self, config: &Config, url: &str) -> Found {
        let Some(index) = KNOWN.iter().position(|(known, ..)| *known == url) else {
            return Err("unknown_app");
        };
        let (known, name, copy) = KNOWN[index];
        if config.fixture {
            return document(known, name, copy.as_bytes()).ok_or("app_unavailable");
        }
        self.resolve(index, || fetch(known)).await
    }

    async fn resolve<F: Future<Output = Result<Vec<u8>>>>(
        &self,
        index: usize,
        fetch: impl FnOnce() -> F,
    ) -> Found {
        let (known, name, copy) = KNOWN[index];
        let entry = || {
            self.fetched
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .get(known)
                .cloned()
                .unwrap_or_default()
        };
        let keep = |entry| {
            self.fetched
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(known, entry);
        };
        // A fetched known document may remove redirects, but it may not silently widen the
        // reviewed embedded redirect boundary. A legitimate addition is picked up only after
        // the embedded copy changes in a reviewed release.
        let parse = |body: &[u8]| known_document(known, name, copy.as_bytes(), body);
        settle(known, &self.fetching[index], entry, keep, parse, fetch).await
    }

    /// A website's or other app's document, at any `https` URL on the public internet, read
    /// as the [`network`] module says. Only for an app the owner lets connect: such an app
    /// is never known. `unknown_app` for a URL no website's client id can be.
    pub async fn website(&self, config: &Config, url: &str) -> Found {
        let Some(target) = network::web_client_id(url) else {
            return Err("unknown_app");
        };
        // Do not queue unbounded HTTP requests behind the active-fetch semaphore or one URL's
        // mutex. A caller that cannot reserve its complete wait returns before cache mutation.
        let _request = self
            .website_requests
            .try_acquire()
            .map_err(|_| "app_unavailable")?;
        let network = self.network(config);
        self.resolve_website(url, || async move {
            let _slot = self
                .website_fetches
                .acquire()
                .await
                .map_err(|_| Error::new("app_unavailable", 502))?;
            network::fetch(network.as_ref(), &target, LARGEST).await
        })
        .await
    }

    async fn resolve_website<F: Future<Output = Result<Vec<u8>>>>(
        &self,
        url: &str,
        fetch: impl FnOnce() -> F,
    ) -> Found {
        let websites = || {
            self.websites
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
        };
        let fetching = {
            let mut websites = websites();
            if !websites.contains_key(url) && websites.len() >= MOST_WEBSITES {
                // The longest unused that no request is fetching makes room.
                let unused = websites
                    .iter()
                    .filter(|(_, website)| Arc::strong_count(&website.fetching) == 1)
                    .min_by_key(|(_, website)| website.asked)
                    .map(|(url, _)| url.clone());
                let Some(unused) = unused else {
                    return Err("app_unavailable");
                };
                websites.remove(&unused);
            }
            let website = websites.entry(url.to_owned()).or_default();
            website.asked = now();
            website.fetching.clone()
        };
        let entry = || {
            websites()
                .get(url)
                .map(|website| website.fetched.clone())
                .unwrap_or_default()
        };
        let keep = |fetched| {
            if let Some(website) = websites().get_mut(url) {
                website.fetched = fetched;
            }
        };
        settle(
            url,
            &fetching,
            entry,
            keep,
            |body| website_document(url, body),
            fetch,
        )
        .await
    }

    /// Tests read websites' documents from `network` instead of the internet.
    pub fn use_network(&self, network: Arc<dyn Network>) {
        *self
            .network
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(network);
    }
    fn network(&self, config: &Config) -> Arc<dyn Network> {
        let chosen = self
            .network
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        chosen.unwrap_or_else(|| {
            if config.fixture {
                Arc::new(network::Fixture)
            } else {
                Arc::new(network::Internet)
            }
        })
    }
}

/// An app's document as `entry` holds it, or fetched under `fetching` when that is not
/// settled, and given to `keep`. Requests that waited for a fetch take its answer.
async fn settle<F: Future<Output = Result<Vec<u8>>>>(
    url: &str,
    fetching: &tokio::sync::Mutex<()>,
    entry: impl Fn() -> Fetched,
    keep: impl FnOnce(Fetched),
    parse: impl FnOnce(&[u8]) -> Option<Client>,
    fetch: impl FnOnce() -> F,
) -> Found {
    if let Some(found) = entry().settled() {
        return found;
    }
    let _only = fetching.lock().await;
    // Whoever fetched while this request waited has settled it.
    let mut entry = entry();
    if let Some(found) = entry.settled() {
        return found;
    }
    let found = match fetch().await.ok().and_then(|body| parse(&body)) {
        Some(client) => {
            entry.document = Some((now(), client.clone()));
            entry.failed = None;
            Ok(client)
        }
        None => {
            observability::event(
                "warn",
                "oauth.client_document_failed",
                json!({"clientId":url}),
            );
            entry.failed = Some(now());
            entry.kept()
        }
    };
    keep(entry);
    found
}

/// Whether `url` is a known app's client document.
pub fn known(url: &str) -> bool {
    KNOWN.iter().any(|(known, ..)| *known == url)
}
/// The known app whose client document is at `url`, as the owner's choice of apps names it.
pub fn known_app(url: &str) -> Option<OAuthAppId> {
    let index = KNOWN.iter().position(|(known, ..)| *known == url)?;
    Some(KNOWN_APPS[index])
}

/// A client document's body: HTTPS only, no redirects, within 5 seconds and 64 KiB.
async fn fetch(url: &str) -> Result<Vec<u8>> {
    let failed = |_| Error::new("app_unavailable", 502);
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .user_agent("Dispatch")
        .build()
        .map_err(failed)?;
    let mut response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(failed)?;
    ensure(response.status() == 200, "app_unavailable", 502)?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        body.extend_from_slice(&chunk);
        ensure(body.len() <= LARGEST, "app_unavailable", 502)?;
    }
    Ok(body)
}

/// A client document as Dispatch uses it: its `client_id` must be the URL it came from and it
/// must list where the app may be sent back to. Only its `https` and loopback redirects
/// count; any other is ignored. How the app says it authenticates is ignored too, since only
/// public clients exist here.
fn document(url: &str, fallback: &str, body: &[u8]) -> Option<Client> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value["client_id"].as_str() != Some(url) {
        return None;
    }
    let redirect_uris: Vec<String> = value["redirect_uris"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|uri| {
            loopback(uri).is_some()
                || url::Url::parse(uri)
                    .is_ok_and(|url| url.scheme() == "https" && url.fragment().is_none())
        })
        .map(str::to_owned)
        .collect();
    if redirect_uris.is_empty() {
        return None;
    }
    Some(Client {
        id: url.to_owned(),
        name: value["client_name"]
            .as_str()
            .and_then(display_name)
            .unwrap_or_else(|| fallback.to_owned()),
        known: true,
        redirect_uris,
    })
}
/// A fetched known document constrained to the redirect boundary reviewed in `embedded`.
fn known_document(url: &str, fallback: &str, embedded: &[u8], fetched: &[u8]) -> Option<Client> {
    let baseline = document(url, fallback, embedded)?;
    let client = document(url, fallback, fetched)?;
    client
        .redirect_uris
        .iter()
        .all(|redirect| allowed(&baseline.redirect_uris, redirect))
        .then_some(client)
}
/// A website's or other app's client document: read as a known app's is, but only ever
/// app-provided metadata, by the name it gives itself or else its host.
fn website_document(url: &str, body: &[u8]) -> Option<Client> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_owned();
    Some(Client {
        known: false,
        ..document(url, &host, body)?
    })
}

/// A name an app gave itself, made fit to show: trimmed, without control or format
/// characters (which could hide or reorder what the owner reads), at most 80 characters.
/// None when nothing is left.
fn display_name(name: &str) -> Option<String> {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !format_character(*c))
        .collect();
    let name: String = clean.trim().chars().take(80).collect();
    let name = name.trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Unicode's format characters (general category Cf): invisible, and some reorder text.
fn format_character(c: char) -> bool {
    matches!(
        u32::from(c),
        0xAD | 0x600..=0x605
            | 0x61C
            | 0x6DD
            | 0x70F
            | 0x890..=0x891
            | 0x8E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Whether `requested` is one of the app's redirects: the same text, or for a loopback `http`
/// redirect the same but for the port, which a native app picks when it starts listening.
pub fn allowed(registered: &[String], requested: &str) -> bool {
    registered.iter().any(|uri| {
        uri == requested || loopback(uri).is_some_and(|parts| Some(parts) == loopback(requested))
    })
}

/// A loopback `http` redirect's host and what follows its port, or None for any other.
fn loopback(uri: &str) -> Option<(&'static str, &str)> {
    let rest = uri.strip_prefix("http://")?;
    let (authority, path) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let host = LOOPBACK.iter().find(|host| {
        authority.strip_prefix(**host).is_some_and(|port| {
            port.is_empty()
                || port.strip_prefix(':').is_some_and(|port| {
                    port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
                })
        })
    })?;
    Some((host, path))
}

/// The private-use scheme (RFC 8252 §7.1) a redirect opens a native app by, or None for any
/// other. Such a scheme is reverse-domain, as `com.example.app`, or a known native app's, as
/// `cursor`. A `web+` scheme is a website's, however a browser hands it on.
pub fn private_scheme(uri: &str) -> Option<&str> {
    let (scheme, rest) = uri.split_once(':')?;
    let valid = scheme
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_lowercase())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b));
    let native = scheme.contains('.') || NATIVE.contains(&scheme);
    (valid
        && native
        && !rest.is_empty()
        && !scheme.starts_with("web+")
        && !NOT_PRIVATE.contains(&scheme))
    .then_some(scheme)
}

/// Whether an app that registered itself may be sent back to `uri`: a loopback `http`
/// address, `http://localhost[:port]/…` and the like, or the app's own private-use scheme.
/// Either opens only an app on the owner's own computer.
fn registrable(uri: &str) -> bool {
    let plain = uri.len() <= LONGEST_REDIRECT
        && uri.is_ascii()
        && !uri.contains('#')
        && !uri.bytes().any(|b| b.is_ascii_control() || b == b' ')
        && url::Url::parse(uri).is_ok();
    plain
        && match loopback(uri) {
            Some((_, path)) => path.starts_with('/'),
            None => private_scheme(uri).is_some(),
        }
}

/// Whether an app that registered itself may be sent back to `uri` as a website: an `https`
/// address written exactly as the URL standard writes it, naming its host by a dotted name,
/// with no credentials or fragment. Only while the owner lets websites and other apps connect.
fn registrable_website(uri: &str) -> bool {
    uri.len() <= LONGEST_REDIRECT
        && url::Url::parse(uri).is_ok_and(|url| {
            url.scheme() == "https"
                && url.as_str() == uri
                && matches!(url.host(), Some(url::Host::Domain(host)) if host.contains('.'))
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
}

/// Where a redirect sends the owner, as the approval page says it: "this computer" for a
/// loopback address or an app's own scheme, with that scheme, and otherwise the host.
pub fn destination(uri: &str) -> (String, Option<String>) {
    if loopback(uri).is_some() {
        return ("this computer".into(), None);
    }
    if let Some(scheme) = private_scheme(uri) {
        return ("this computer".into(), Some(scheme.to_owned()));
    }
    let host = url::Url::parse(uri)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    (host, None)
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
            .and_then(display_name)
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

#[cfg(test)]
#[path = "../../tests/backend/oauth/clients.rs"]
mod tests;
