//! The apps that may ask to connect. Four known apps by their published client documents
//! (CIMD), fetched from here and never from a browser; and apps that register themselves
//! (RFC 7591), which may only send the owner back to an app on the owner's own computer.
use super::{Answer, Refusal};
use crate::{
    Error, Result,
    config::Config,
    crypto,
    db::{Store, at, iso, now},
    ensure, observability,
};
use serde_json::{Value, json};
use std::{collections::HashMap, future::Future, sync::Mutex, time::Duration};

/// An app as an authorization names it: its client id, the name it goes by, whether Dispatch
/// knows it or only has its word, and where it may be sent back to.
#[derive(Clone, Debug)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub verified: bool,
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
/// app runs at a time; the requests that wait for it take its answer.
#[derive(Default)]
pub struct Documents {
    fetched: Mutex<HashMap<&'static str, Fetched>>,
    fetching: [tokio::sync::Mutex<()>; KNOWN.len()],
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
        let (known, name, _) = KNOWN[index];
        let entry = || {
            self.fetched
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .get(known)
                .cloned()
                .unwrap_or_default()
        };
        if let Some(found) = entry().settled() {
            return found;
        }
        let _only = self.fetching[index].lock().await;
        // Whoever fetched while this request waited has settled it.
        let mut entry = entry();
        if let Some(found) = entry.settled() {
            return found;
        }
        let found = match fetch()
            .await
            .ok()
            .and_then(|body| document(known, name, &body))
        {
            Some(client) => {
                entry.document = Some((now(), client.clone()));
                entry.failed = None;
                Ok(client)
            }
            None => {
                observability::event(
                    "warn",
                    "oauth.client_document_failed",
                    json!({"clientId":known}),
                );
                entry.failed = Some(now());
                entry.kept()
            }
        };
        self.fetched
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(known, entry);
        found
    }
}

/// Whether `url` is a known app's client document.
pub fn known(url: &str) -> bool {
    KNOWN.iter().any(|(known, ..)| *known == url)
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
        verified: true,
        redirect_uris,
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

impl Store {
    /// An app that registered itself, while it is still registered.
    pub(super) fn registered_client(&self, id: &str) -> Result<Option<Client>> {
        let Some(row) = self.platform.one(
            "SELECT name,redirect_uris FROM oauth_clients WHERE id=?",
            [id],
        )?
        else {
            return Ok(None);
        };
        Ok(Some(Client {
            id: id.to_owned(),
            name: crate::db::s(&row, "name").to_owned(),
            verified: false,
            redirect_uris: serde_json::from_str(crate::db::s(&row, "redirect_uris"))?,
        }))
    }

    /// Registers an app (RFC 7591): only a public client, whose every redirect opens an app on
    /// the owner's own computer. Answers the registration as the client is to keep it.
    pub fn register_oauth_client(&self, metadata: &Value) -> Result<Answer<Value>> {
        let refused = |error, description: &str| Ok(Err(Refusal::new(error, description)));
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
        if redirect_uris.is_empty()
            || redirect_uris.len() > MOST_REDIRECTS
            || !redirect_uris.iter().all(|uri| registrable(uri))
        {
            return refused(
                "invalid_redirect_uri",
                "Up to 5 redirects, each a loopback http address or the app's own URI scheme",
            );
        }
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

    /// Notes that a registered app was given tokens, which keeps its registration.
    pub(super) fn used_oauth_client(&self, id: &str) -> Result<()> {
        self.platform.exec(
            "UPDATE oauth_clients SET last_used_at=? WHERE id=?",
            [iso(), id.to_owned()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_documents_are_the_known_apps_own() {
        for (url, name, copy) in KNOWN {
            let client = document(url, "Fallback", copy.as_bytes()).unwrap();
            assert_eq!((client.name.as_str(), client.verified), (*name, true));
            assert!(!client.redirect_uris.is_empty());
        }
        // A document naming another client, or none of its redirects, is no document.
        let (url, _, copy) = KNOWN[0];
        assert!(document("https://chatgpt.com/oauth/other.json", "x", copy.as_bytes()).is_none());
        let bare = json!({"client_id":url,"redirect_uris":[]}).to_string();
        assert!(document(url, "x", bare.as_bytes()).is_none());
        let unnamed = json!({"client_id":url,"redirect_uris":["https://a.example/cb"]});
        let client = document(url, "ChatGPT", unnamed.to_string().as_bytes()).unwrap();
        assert_eq!(client.name, "ChatGPT");
        assert!(document(url, "x", b"<html>").is_none());
        // Only https and loopback redirects count; a document listing none is no document.
        let mixed = json!({"client_id":url,"redirect_uris":[
            "javascript:alert(1)", "http://evil.example/cb", "myapp.example://cb",
            "https://a.example/cb", "https://a.example/cb#x", "http://127.0.0.1/cb", 7]});
        let client = document(url, "x", mixed.to_string().as_bytes()).unwrap();
        assert_eq!(
            client.redirect_uris,
            ["https://a.example/cb", "http://127.0.0.1/cb"]
        );
        let none = json!({"client_id":url,"redirect_uris":["cursor://x/cb"]});
        assert!(document(url, "x", none.to_string().as_bytes()).is_none());
    }

    fn fetched(body: &str) -> impl Future<Output = Result<Vec<u8>>> {
        let body = body.to_owned();
        async move { Ok(body.into_bytes()) }
    }
    async fn failed() -> Result<Vec<u8>> {
        Err(Error::new("app_unavailable", 502))
    }

    #[tokio::test]
    async fn documents_are_fetched_once_at_a_time_and_failures_wait_a_minute() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (_, _, copy) = KNOWN[1];
        let documents = Documents::default();
        let fetches = AtomicUsize::new(0);
        // Requests arriving together share one fetch.
        let slow = || {
            fetches.fetch_add(1, Ordering::SeqCst);
            async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(copy.as_bytes().to_vec())
            }
        };
        let (a, b, c) = tokio::join!(
            documents.resolve(1, slow),
            documents.resolve(1, slow),
            documents.resolve(1, slow)
        );
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        assert!(a.is_ok() && b.is_ok() && c.is_ok());
        // Fresh for an hour: no fetch at all.
        let found = documents.resolve(1, failed).await.unwrap();
        assert_eq!(found.name, "Codex");
        // A failed fetch is not tried again for a minute, and nothing is fetched meanwhile.
        let other = Documents::default();
        assert_eq!(
            other.resolve(2, failed).await.unwrap_err(),
            "app_unavailable"
        );
        let tried = AtomicUsize::new(0);
        let counted = || {
            tried.fetch_add(1, Ordering::SeqCst);
            fetched(copy)
        };
        assert_eq!(
            other.resolve(2, counted).await.unwrap_err(),
            "app_unavailable"
        );
        assert_eq!(tried.load(Ordering::SeqCst), 0);
        // After the minute it is fetched again.
        let set = |document: &Documents, entry: Fetched| {
            document.fetched.lock().unwrap().insert(KNOWN[2].0, entry);
        };
        set(
            &other,
            Fetched {
                document: None,
                failed: Some(now() - RETRY - 1),
            },
        );
        let (url, name, copy) = KNOWN[2];
        assert_eq!(other.resolve(2, || fetched(copy)).await.unwrap().id, url);
        // A stale document that cannot be fetched again still serves its day.
        let stale = document(url, name, copy.as_bytes()).unwrap();
        set(
            &other,
            Fetched {
                document: Some((now() - FRESH - 1, stale.clone())),
                failed: None,
            },
        );
        assert_eq!(other.resolve(2, failed).await.unwrap().id, url);
        set(
            &other,
            Fetched {
                document: Some((now() - KEPT - 1, stale)),
                failed: None,
            },
        );
        assert_eq!(
            other.resolve(2, failed).await.unwrap_err(),
            "app_unavailable"
        );
    }

    #[test]
    fn loopback_redirects_match_on_any_port_and_nothing_else_does() {
        let registered = |uris: &[&str]| uris.iter().map(|u| (*u).to_owned()).collect::<Vec<_>>();
        let portless = registered(&["http://localhost/callback", "http://127.0.0.1/callback"]);
        assert!(allowed(&portless, "http://localhost:53122/callback"));
        assert!(allowed(&portless, "http://127.0.0.1:61001/callback"));
        assert!(allowed(&portless, "http://127.0.0.1/callback"));
        for wrong in [
            "http://127.0.0.1:61001/callback2",
            "http://127.0.0.1:61001/callback?x=1",
            "https://127.0.0.1:61001/callback",
            "http://127.0.0.2:61001/callback",
            "http://127.0.0.1:99999/callback",
            "http://127.0.0.1:/callback",
            "http://localhost.evil.example/callback",
            "http://localhost@evil.example/callback",
        ] {
            assert!(!allowed(&portless, wrong), "{wrong}");
        }
        // localhost and 127.0.0.1 are different hosts.
        assert!(!allowed(
            &registered(&["http://localhost/cb"]),
            "http://127.0.0.1:5/cb"
        ));
        assert!(allowed(
            &registered(&["http://[::1]:7/cb"]),
            "http://[::1]:9/cb"
        ));
        // Anything else matches only exactly, port and all.
        let web = registered(&["https://chatgpt.com/connector_platform_oauth_redirect"]);
        assert!(allowed(
            &web,
            "https://chatgpt.com/connector_platform_oauth_redirect"
        ));
        assert!(!allowed(
            &web,
            "https://chatgpt.com:443/connector_platform_oauth_redirect"
        ));
        assert!(!allowed(
            &web,
            "https://chatgpt.com/connector_platform_oauth_redirect/"
        ));
        let app = registered(&["cursor://anysphere.cursor-retrieval/oauth/callback"]);
        assert!(allowed(
            &app,
            "cursor://anysphere.cursor-retrieval/oauth/callback"
        ));
        assert!(!allowed(
            &app,
            "cursor://anysphere.cursor-retrieval:1/oauth/callback"
        ));
    }

    #[test]
    fn apps_register_only_redirects_to_this_computer() {
        for uri in [
            "http://127.0.0.1:27890/callback",
            "http://localhost/callback",
            "http://[::1]:8080/cb?x=1",
            "cursor://anysphere.cursor-retrieval/oauth/callback",
            "vscode://vscode.github-authentication/did-authenticate",
            "vscode-insiders://callback",
            "windsurf://codeium.windsurf/callback",
            "com.example.app:/oauth2redirect",
        ] {
            assert!(registrable(uri), "{uri}");
        }
        // A scheme is a native app's only when reverse-domain or a known app's; a web+ scheme
        // is a website's protocol handler.
        for uri in [
            "web+dsp://cb",
            "web+dsp.example://cb",
            "magnet:?xt=urn:btih:abc",
            "myapp://callback",
            "sms:5550100",
        ] {
            assert!(private_scheme(uri).is_none(), "{uri}");
            assert!(!registrable(uri), "{uri}");
        }
        let long = format!("http://127.0.0.1:1/{}", "a".repeat(LONGEST_REDIRECT));
        assert!(!registrable(&long));
        for uri in [
            "https://evil.example/callback",
            "http://evil.example/callback",
            "http://127.0.0.1:27890",
            "http://127.0.0.1:27890/callback#here",
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/passwd",
            "JavaScript:alert(1)",
            "Cursor://x/y",
            "1app://x",
            "cursor:",
            "cursor://x/ y",
        ] {
            assert!(!registrable(uri), "{uri}");
        }
        assert_eq!(
            destination("http://127.0.0.1:5/callback"),
            ("this computer".into(), None)
        );
        assert_eq!(
            destination("cursor://anysphere.cursor-retrieval/oauth/callback"),
            ("this computer".into(), Some("cursor".into()))
        );
        assert_eq!(
            destination("https://chatgpt.com/connector_platform_oauth_redirect"),
            ("chatgpt.com".into(), None)
        );
    }

    #[test]
    fn names_are_trimmed_and_cut_short() {
        assert_eq!(display_name("  Codex\n "), Some("Codex".into()));
        assert_eq!(display_name(&"a".repeat(100)).unwrap().len(), 80);
        assert_eq!(display_name(" \u{7} "), None);
        // Invisible and reordering characters cannot hide or disguise a name.
        assert_eq!(
            display_name("\u{202E}edoC \u{200B}edualC\u{2066}\u{FEFF}"),
            Some("edoC edualC".into())
        );
        assert_eq!(display_name("\u{200F}\u{2069}\u{E0041}"), None);
    }
}
