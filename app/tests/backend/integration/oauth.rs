//! Sign in with Dispatch, over HTTP against the real router: discovery, the authorization
//! request and its refusals, the owner's approval, the token endpoint, revocation, and the
//! connected app signing in to the agent API and MCP like a key.
use dispatch_core::testing as common;
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    mcp::{
        api::types::AgentArea,
        oauth::network::{Network, Pending},
    },
    server::operations,
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

/// A kind of data a key may read, by its id.
fn kind(id: &str) -> AgentArea {
    AgentArea::parse(id).unwrap()
}

const CLAUDE_CODE: &str = "https://claude.ai/oauth/claude-code-client-metadata";
const CHATGPT: &str = "https://chatgpt.com/oauth/client.json";
const CHATGPT_REDIRECT: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

struct Server {
    _root: tempfile::TempDir,
    origin: String,
    state: Arc<State>,
    client: reqwest::Client,
    /// The cookies the owner's browser keeps for the requests it brought from apps, one per
    /// request as `name=nonce`: set by the authorization endpoint, cleared by an answer, sent
    /// with the owner's calls.
    browser: std::sync::Mutex<Vec<String>>,
}
/// The name of a browser's cookie for an authorization request, as development names it.
fn browser_cookie(request: &str) -> String {
    format!("dispatch_oauth_request_{request}")
}
struct Owner {
    cookie: String,
    csrf: String,
}
struct Answer {
    status: u16,
    body: Value,
    headers: reqwest::header::HeaderMap,
}
impl Answer {
    fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
    /// Where a redirect goes: the Location's query parameters, after its `?`.
    fn location(&self) -> Vec<(String, String)> {
        let location = self.header("location");
        let query = location.split_once('?').map_or("", |(_, query)| query);
        url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect()
    }
}
fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}
impl Server {
    async fn start() -> Self {
        dispatch_backend::install();
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let dashboard = root.path().join("dashboard");
        std::fs::create_dir_all(dashboard.join("assets")).unwrap();
        std::fs::write(dashboard.join("index.html"), "<html><head></head></html>").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = Config::load().unwrap();
        config.root = root.path().join("state");
        db::private_dir(&config.root).unwrap();
        config.development = true;
        config.fixture = true;
        config.environment = "preview".into();
        config.standalone = true;
        config.dashboard = dashboard;
        config.port = port;
        config.origin = format!("http://127.0.0.1:{port}");
        operations::seed(&Store::initialize(config.clone()).unwrap()).unwrap();
        let state = State::new(config).unwrap();
        let app = dispatch_core::server::http::router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            browser: std::sync::Mutex::default(),
            _root: root,
            origin: format!("http://127.0.0.1:{port}"),
            state,
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        }
    }
    /// A server whose platform owner has opened the pairing window, as apps need to ask.
    async fn paired() -> Self {
        let server = Self::start().await;
        server.open_pairing().await;
        server
    }
    /// The platform owner opens the pairing window, as the Connect tab does.
    async fn open_pairing(&self) {
        self.state
            .run(|db| {
                let (owner,): (String,) = db
                    .platform
                    .one_as("SELECT id FROM users WHERE email='owner@dispatch.test'", [])?
                    .unwrap();
                db.open_oauth_pairing(&owner)
            })
            .await
            .unwrap();
    }
    /// A session row written directly, so tests do not pay for password hashing.
    async fn owner(&self) -> Owner {
        self.signed_in(db::now()).await
    }
    /// The platform owner's session, signed in at `created_at`.
    async fn signed_in(&self, created_at: i64) -> Owner {
        let raw = crypto::token().unwrap();
        let token = raw.clone();
        self.state
            .run(move |db| {
                let user = db
                    .platform
                    .one(
                        "SELECT id,version FROM users WHERE email='owner@dispatch.test'",
                        [],
                    )?
                    .unwrap();
                db.platform.exec(
                    "INSERT INTO sessions VALUES (?,?,?,?,?)",
                    rusqlite::params![
                        crypto::sha(&token),
                        s(&user, "id"),
                        db::n(&user, "version"),
                        db::now() + 600000,
                        created_at
                    ],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let cookie = format!("dispatch_session={raw}");
        let session = self
            .send(
                self.client
                    .get(self.url("/api/session"))
                    .header("cookie", &cookie),
            )
            .await;
        assert_eq!(session.status, 200, "{}", session.body);
        Owner {
            cookie,
            csrf: s(&session.body, "csrf").to_owned(),
        }
    }
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }
    async fn send(&self, request: reqwest::RequestBuilder) -> Answer {
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.unwrap();
        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            headers,
        }
    }
    async fn get(&self, path: &str) -> Answer {
        self.send(self.client.get(self.url(path))).await
    }
    async fn text(&self, path: &str) -> (u16, String) {
        let response = self.client.get(self.url(path)).send().await.unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }
    async fn as_owner(&self, owner: &Owner, method: &str, path: &str, body: Value) -> Answer {
        let request = if method == "GET" {
            self.client.get(self.url(path))
        } else {
            self.client
                .post(self.url(path))
                .header("origin", &self.origin)
                .header("content-type", "application/json")
                .header("x-csrf-token", &owner.csrf)
                .body(body.to_string())
        };
        let cookie = [owner.cookie.clone()]
            .into_iter()
            .chain(self.browser())
            .collect::<Vec<_>>()
            .join("; ");
        let answer = self.send(request.header("cookie", cookie)).await;
        self.keep_browser(&answer);
        answer
    }
    /// The owner's browser's cookies for its requests, as it would send them.
    fn browser(&self) -> Vec<String> {
        self.browser.lock().unwrap().clone()
    }
    fn set_browser(&self, cookies: &[String]) {
        *self.browser.lock().unwrap() = cookies.to_vec();
    }
    /// Keeps or clears a request's cookie as an answer says, as a browser does, leaving the
    /// others it holds alone.
    fn keep_browser(&self, answer: &Answer) {
        for value in answer.headers.get_all("set-cookie") {
            let value = value.to_str().unwrap();
            let pair = value.split(';').next().unwrap();
            let Some((name, nonce)) = pair.split_once('=') else {
                continue;
            };
            if !name.starts_with("dispatch_oauth_request_") {
                continue;
            }
            let mut held = self.browser.lock().unwrap();
            held.retain(|cookie| !cookie.starts_with(&format!("{name}=")));
            if !nonce.is_empty() {
                held.push(pair.to_owned());
            }
        }
    }
    async fn form(&self, path: &str, fields: &[(&str, &str)]) -> Answer {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        self.send(
            self.client
                .post(self.url(path))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body),
        )
        .await
    }
    async fn authorize(&self, fields: &[(&str, &str)]) -> Answer {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        let answer = self.get(&format!("/oauth/authorize?{query}")).await;
        // The owner's browser follows the app's link.
        self.keep_browser(&answer);
        answer
    }
    async fn dsp(&self, name: &'static str) -> String {
        self.state
            .read(move |db| {
                let row = db
                    .platform
                    .one("SELECT id FROM dsps WHERE name=?", [name])?;
                Ok(s(&row.unwrap(), "id").to_owned())
            })
            .await
            .unwrap()
    }
    fn resource(&self) -> String {
        format!("{}/api/v1/mcp", self.origin)
    }
    /// An authorization request as a client sends one, with its PKCE challenge and state.
    fn request<'a>(
        &'a self,
        client: &'a str,
        redirect: &'a str,
        challenge: &'a str,
    ) -> Vec<(&'a str, String)> {
        vec![
            ("response_type", "code".into()),
            ("client_id", client.into()),
            ("redirect_uri", redirect.into()),
            ("code_challenge", challenge.into()),
            ("code_challenge_method", "S256".into()),
            ("state", "st/ate 1".into()),
            ("resource", self.resource()),
            ("scope", "dispatch offline_access".into()),
            ("ui_locales", "en".into()),
        ]
    }
    /// The approval page's request id from an authorization's redirect.
    async fn requested(&self, client: &str, redirect: &str) -> String {
        let challenge = crypto::s256(VERIFIER);
        let fields = self.request(client, redirect, &challenge);
        let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let answer = self.authorize(&pairs).await;
        assert_eq!(answer.status, 302, "{}", answer.body);
        let prefix = format!("{}/#authorize?request=", self.origin);
        answer
            .header("location")
            .strip_prefix(&prefix)
            .unwrap_or_else(|| panic!("{}", answer.header("location")))
            .to_owned()
    }
    /// The code an approval sends back, checked to carry the state and the issuer.
    async fn approved(&self, owner: &Owner, request: &str, choices: Value) -> String {
        let approved = self
            .as_owner(
                owner,
                "POST",
                &format!("/api/platform/oauth/requests/{request}/approve"),
                choices,
            )
            .await;
        assert_eq!(approved.status, 200, "{}", approved.body);
        let redirect = s(&approved.body, "redirect");
        let query = redirect.split_once('?').unwrap().1;
        let params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(param(&params, "state"), Some("st/ate 1"));
        assert_eq!(param(&params, "iss"), Some(self.origin.as_str()));
        param(&params, "code").unwrap().to_owned()
    }
    async fn exchange(&self, client: &str, redirect: &str, code: &str) -> Answer {
        self.form(
            "/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect),
                ("code_verifier", VERIFIER),
                ("client_id", client),
                ("resource", &self.resource()),
            ],
        )
        .await
    }
    async fn refresh(&self, client: &str, token: &str) -> Answer {
        self.form(
            "/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", token),
                ("client_id", client),
                ("resource", &self.resource()),
                ("scope", "dispatch offline_access"),
            ],
        )
        .await
    }
    /// A connected app from authorization to its first tokens.
    async fn connect(&self, owner: &Owner, client: &str, redirect: &str, choices: Value) -> Value {
        let request = self.requested(client, redirect).await;
        let code = self.approved(owner, &request, choices).await;
        let tokens = self.exchange(client, redirect, &code).await;
        assert_eq!(tokens.status, 200, "{}", tokens.body);
        tokens.body
    }
    async fn bearer(&self, path: &str, token: &str) -> Answer {
        self.send(
            self.client
                .get(self.url(path))
                .header("authorization", format!("Bearer {token}")),
        )
        .await
    }
    async fn mcp(&self, token: &str, method: &str) -> Answer {
        self.send(
            self.client
                .post(self.url("/api/v1/mcp"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-06-18")
                .body(json!({"jsonrpc":"2.0","id":1,"method":method,"params":{}}).to_string()),
        )
        .await
    }
}
/// Every kind of data there is, delivery addresses included.
const EVERY: &[&str] = &[
    "routes",
    "locations",
    "timecards",
    "meal_breaks",
    "dvic",
    "feedback",
    "safety",
    "returns",
    "scorecard",
];
fn everything(name: &str) -> Value {
    json!({"name":name,"allDsps":true,"dsps":[],"reads":{"areas":EVERY,"bypass":false}})
}

#[tokio::test]
async fn discovery_documents_name_the_issuer_and_resource_byte_for_byte() {
    let server = Server::start().await;
    let origin = &server.origin;
    let resource = format!(
        "{{\"resource\":\"{origin}/api/v1/mcp\",\"authorization_servers\":[\"{origin}\"],\
         \"scopes_supported\":[\"dispatch\"],\"bearer_methods_supported\":[\"header\"],\
         \"resource_name\":\"Dispatch\"}}"
    );
    let metadata = format!(
        "{{\"issuer\":\"{origin}\",\"authorization_endpoint\":\"{origin}/oauth/authorize\",\
         \"token_endpoint\":\"{origin}/oauth/token\",\"registration_endpoint\":\"{origin}/oauth/register\",\
         \"revocation_endpoint\":\"{origin}/oauth/revoke\",\"response_types_supported\":[\"code\"],\
         \"response_modes_supported\":[\"query\"],\
         \"grant_types_supported\":[\"authorization_code\",\"refresh_token\"],\
         \"code_challenge_methods_supported\":[\"S256\"],\
         \"token_endpoint_auth_methods_supported\":[\"none\"],\
         \"revocation_endpoint_auth_methods_supported\":[\"none\"],\
         \"scopes_supported\":[\"dispatch\"],\"client_id_metadata_document_supported\":true,\
         \"authorization_response_iss_parameter_supported\":true}}"
    );
    assert!(!origin.ends_with('/'));
    for (path, expected) in [
        ("/.well-known/oauth-protected-resource", &resource),
        (
            "/.well-known/oauth-protected-resource/api/v1/mcp",
            &resource,
        ),
        ("/.well-known/oauth-authorization-server", &metadata),
        (
            "/.well-known/oauth-authorization-server/api/v1/mcp",
            &metadata,
        ),
    ] {
        assert_eq!(server.text(path).await, (200, expected.clone()), "{path}");
    }
    // Answered from memory, before the query is read.
    assert_eq!(
        server
            .get("/.well-known/oauth-authorization-server?a=1&a=2")
            .await
            .status,
        200
    );
    // No OpenID document, and nothing unknown under .well-known is the dashboard's page.
    for path in [
        "/.well-known/openid-configuration",
        "/.well-known/openid-configuration/api/v1/mcp",
        "/api/v1/mcp/.well-known/openid-configuration",
        "/.well-known/anything",
        "/oauth/anything",
    ] {
        let answer = server.get(path).await;
        assert!([401, 404].contains(&answer.status), "{path}");
        assert!(answer.body.is_object(), "{path}");
    }
}

#[tokio::test]
async fn the_mcp_endpoint_challenges_with_where_to_sign_in() {
    let server = Server::start().await;
    let challenge = format!(
        "Bearer realm=\"Dispatch\", resource_metadata=\"{}/.well-known/oauth-protected-resource/api/v1/mcp\", \
         scope=\"dispatch\"",
        server.origin
    );
    // No token: no error, so the client starts signing in.
    for answer in [
        server.get("/api/v1/mcp").await,
        server
            .send(
                server
                    .client
                    .post(server.url("/api/v1/mcp"))
                    .header("content-type", "application/json")
                    .body("{}"),
            )
            .await,
        server.get("/api/v1/whoami").await,
    ] {
        assert_eq!(answer.status, 401);
        assert_eq!(answer.header("www-authenticate"), challenge);
    }
    // A token that is no good: the client refreshes or signs in again.
    for token in ["dsa_dev_nothing", "dsk_dev_nothing", "dsr_dev_nothing"] {
        let answer = server.bearer("/api/v1/mcp", token).await;
        assert_eq!(answer.status, 401, "{token}");
        assert_eq!(
            answer.header("www-authenticate"),
            format!("{challenge}, error=\"invalid_token\""),
            "{token}"
        );
    }
}

#[tokio::test]
async fn refused_apps_and_redirects_never_go_back_to_the_app() {
    let server = Server::paired().await;
    let challenge = crypto::s256(VERIFIER);
    let page = |error: &str| format!("{}/#authorize?error={error}", server.origin);
    let local = "http://localhost:61234/callback";
    let cases = [
        // Not a known app, a known app's lookalike, or no app at all.
        (
            Some("https://evil.example/client.json"),
            Some(local),
            "unknown_app",
        ),
        (
            Some("https://claude.ai/oauth/claude-code-client-metadata/"),
            Some(local),
            "unknown_app",
        ),
        (
            Some("dcr_00000000000000000000000000000000"),
            Some(local),
            "unknown_app",
        ),
        (Some("evil"), Some(local), "unknown_app"),
        (None, Some(local), "unknown_app"),
        // A redirect the app never registered, or none.
        (
            Some(CLAUDE_CODE),
            Some("http://localhost:61234/other"),
            "invalid_redirect",
        ),
        (
            Some(CLAUDE_CODE),
            Some("https://evil.example/callback"),
            "invalid_redirect",
        ),
        (
            Some(CLAUDE_CODE),
            Some("http://127.0.0.2:61234/callback"),
            "invalid_redirect",
        ),
        (
            Some(CHATGPT),
            Some("https://chatgpt.com/connector_platform_oauth_redirect/"),
            "invalid_redirect",
        ),
        (Some(CLAUDE_CODE), None, "invalid_redirect"),
    ];
    for (client, redirect, error) in cases {
        let mut fields = vec![
            ("response_type", "code"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", "s"),
        ];
        fields.extend(client.map(|client| ("client_id", client)));
        fields.extend(redirect.map(|redirect| ("redirect_uri", redirect)));
        let answer = server.authorize(&fields).await;
        assert_eq!(
            (answer.status, answer.header("location")),
            (302, page(error).as_str()),
            "{client:?} {redirect:?}"
        );
    }
    // Sent twice, the client or its redirect is no request at all.
    let answer = server
        .authorize(&[
            ("client_id", CLAUDE_CODE),
            ("client_id", CLAUDE_CODE),
            ("redirect_uri", local),
        ])
        .await;
    assert_eq!(answer.header("location"), page("unknown_app"));
}

#[tokio::test]
async fn a_known_app_hears_what_was_wrong_with_its_request() {
    let server = Server::paired().await;
    let local = "http://localhost:61234/callback";
    let challenge = crypto::s256(VERIFIER);
    let base = server.request(CLAUDE_CODE, local, &challenge);
    let resource = server.resource();
    let with = |change: &[(&str, Option<&str>)]| {
        let mut fields: Vec<(String, String)> = base
            .iter()
            .filter(|(key, _)| !change.iter().any(|(name, _)| name == key))
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        for (name, value) in change {
            if let Some(value) = value {
                fields.push(((*name).to_owned(), (*value).to_owned()));
            }
        }
        fields
    };
    let cases = [
        (
            with(&[("response_type", Some("token"))]),
            "unsupported_response_type",
        ),
        (
            with(&[("response_type", None)]),
            "unsupported_response_type",
        ),
        (with(&[("code_challenge", None)]), "invalid_request"),
        (with(&[("code_challenge_method", None)]), "invalid_request"),
        (
            with(&[("code_challenge_method", Some("plain"))]),
            "invalid_request",
        ),
        (
            with(&[("code_challenge", Some(&challenge[..42]))]),
            "invalid_request",
        ),
        (
            with(&[("resource", Some(&format!("{resource}/")))]),
            "invalid_target",
        ),
        (
            with(&[("resource", Some(&server.origin))]),
            "invalid_target",
        ),
    ];
    for (fields, error) in cases {
        let pairs: Vec<(&str, &str)> = fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let answer = server.authorize(&pairs).await;
        assert_eq!(answer.status, 302);
        let location = answer.header("location");
        assert!(location.starts_with(&format!("{local}?")), "{location}");
        let params = answer.location();
        assert_eq!(param(&params, "error"), Some(error), "{location}");
        assert!(param(&params, "error_description").is_some());
        assert_eq!(param(&params, "state"), Some("st/ate 1"));
        assert_eq!(param(&params, "iss"), Some(server.origin.as_str()));
    }
    // The resource repeated, as RFC 8707 allows, is fine while it is the same one; a state
    // sent twice is refused, and the app is not handed either.
    let mut twice = with(&[]);
    twice.push(("resource".into(), resource.clone()));
    let pairs: Vec<(&str, &str)> = twice
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let answer = server.authorize(&pairs).await;
    assert!(answer.header("location").contains("#authorize?request="));
    twice.push(("state".into(), "other".into()));
    let pairs: Vec<(&str, &str)> = twice
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let params = server.authorize(&pairs).await.location();
    assert_eq!(param(&params, "error"), Some("invalid_request"));
    assert_eq!(param(&params, "state"), None);
    // MCP clients must name the protected resource. Scope may be omitted and defaults to the
    // one Dispatch grants; an unknown scope is never silently ignored.
    let fields = with(&[("resource", None), ("scope", None)]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let missing = server.authorize(&pairs).await.location();
    assert_eq!(param(&missing, "error"), Some("invalid_target"));
    let fields = with(&[("scope", None)]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert!(
        server
            .authorize(&pairs)
            .await
            .header("location")
            .contains("#authorize?request=")
    );
    let fields = with(&[("scope", Some("dispatch other"))]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let unknown = server.authorize(&pairs).await.location();
    assert_eq!(param(&unknown, "error"), Some("invalid_scope"));
    let fields = with(&[("scope", Some("offline_access"))]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let incomplete = server.authorize(&pairs).await.location();
    assert_eq!(param(&incomplete, "error"), Some("invalid_scope"));
}

#[tokio::test]
async fn the_owner_approves_once_and_the_code_is_redeemed_once() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    // Claude Code's document lists portless loopback redirects; any port is the same one.
    let local = "http://localhost:61234/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{request}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    assert_eq!(
        shown.body["app"],
        json!({"name":"Claude Code","clientId":CLAUDE_CODE,"known":true,
            "redirectHost":"this computer","redirectScheme":null})
    );
    assert_eq!(shown.body["id"], request);
    assert_eq!(shown.body["replaces"], Value::Null);
    assert!(s(&shown.body, "expiresAt") > db::iso().as_str());
    // Nobody else sees or answers it.
    assert_eq!(server.get(&path).await.status, 401);
    let mut bad = everything("Claude Code");
    bad["access"] = json!("operator");
    let refused = server
        .as_owner(&owner, "POST", &format!("{path}/approve"), bad)
        .await;
    assert_eq!(refused.status, 400, "{}", refused.body);
    let held = server.browser();
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    // Answered, the request is gone: the browser no longer holds it, and even its old
    // cookie finds nothing.
    assert!(server.browser().is_empty());
    let answer = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(
        (answer.status, s(&answer.body, "error")),
        (403, "wrong_browser")
    );
    server.set_browser(&held);
    for (method, suffix, body) in [
        ("GET", "", json!({})),
        ("POST", "/approve", everything("Again")),
        ("POST", "/deny", json!({})),
    ] {
        let answer = server
            .as_owner(&owner, method, &format!("{path}{suffix}"), body)
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (404, "authorization_not_found"),
            "{method} {suffix}"
        );
    }
    // The code is bound to the client, the exact redirect and the verifier.
    for (client, redirect, verifier) in [
        (CHATGPT, local, VERIFIER),
        (CLAUDE_CODE, "http://localhost:61235/callback", VERIFIER),
        (CLAUDE_CODE, local, &"x".repeat(43)[..]),
    ] {
        let answer = server
            .form(
                "/oauth/token",
                &[
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", redirect),
                    ("code_verifier", verifier),
                    ("client_id", client),
                    ("resource", &server.resource()),
                ],
            )
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (400, "invalid_grant"),
            "{client} {redirect}"
        );
    }
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    assert_eq!(tokens.header("cache-control"), "no-store");
    assert_eq!(tokens.header("pragma"), "no-cache");
    assert!(
        tokens
            .header("content-type")
            .starts_with("application/json")
    );
    let access = s(&tokens.body, "access_token").to_owned();
    assert!(access.starts_with("dsa_dev_"));
    assert!(s(&tokens.body, "refresh_token").starts_with("dsr_dev_"));
    assert_eq!(
        (
            &tokens.body["token_type"],
            &tokens.body["expires_in"],
            &tokens.body["scope"]
        ),
        (&json!("Bearer"), &json!(3600), &json!("dispatch"))
    );
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
    // The connected app is listed with the keys, as an app, and logged on the platform.
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let app = &listed.body["keys"][0];
    assert_eq!(app["kind"], "app");
    assert_eq!(
        app["client"],
        json!({"name":"Claude Code","known":true,"status":"connected"})
    );
    assert_eq!(
        (app["name"].as_str(), app["hint"].as_str()),
        (Some("Claude Code"), Some(""))
    );
    let log = server
        .state
        .read(|db| common::audits(db, None))
        .await
        .unwrap();
    assert_eq!(s(&log[0], "action"), "agent.app_connected");
    assert_eq!(s(&log[0], "target"), "Claude Code");
    // Redeemed again with everything right, the code ends what it made.
    let again = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(s(&again.body, "error"), "invalid_grant");
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 401);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(listed.body["keys"][0]["revokedAt"].is_string());
}

#[tokio::test]
async fn connecting_an_app_again_replaces_its_earlier_connection() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50002/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    // Unused for 30 days, the app is signed out but still listed and counted.
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_tokens SET expires_at=? WHERE kind='refresh'",
                [db::at(db::now() - 1000)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let apps = |listed: &Answer| -> Vec<Value> {
        listed.body["keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|key| key["kind"] == "app")
            .cloned()
            .collect()
    };
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let earlier = apps(&listed)[0].clone();
    assert_eq!(earlier["client"]["status"], "signed_out");
    assert!(earlier["revokedAt"].is_null());
    // Connected again under the same name, whatever its case: the earlier one gives way.
    let second = server
        .connect(&owner, CLAUDE_CODE, local, everything("claude code"))
        .await;
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&first, "access_token"))
            .await
            .status,
        401
    );
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second, "access_token"))
            .await
            .status,
        200
    );
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let apps = apps(&listed);
    assert_eq!(apps.len(), 2);
    let live: Vec<&Value> = apps
        .iter()
        .filter(|app| app["revokedAt"].is_null())
        .collect();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0]["name"], "claude code");
    assert_eq!(live[0]["client"]["status"], "connected");
    assert_ne!(live[0]["id"], earlier["id"]);
    let ended = apps.iter().find(|app| app["id"] == earlier["id"]).unwrap();
    assert!(ended["revokedAt"].is_string());
    // The log says the owner replaced it, in the step that connected its successor.
    let log = server
        .state
        .read(|db| common::audits(db, None))
        .await
        .unwrap();
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|event| s(event, "action"))
        .filter(|action| action.starts_with("agent."))
        .collect();
    // Newest first, after the owner opened the pairing window for the first connection.
    assert_eq!(
        actions,
        [
            "agent.app_connected",
            "agent.app_revoked",
            "agent.app_connected",
            "agent.pairing_opened"
        ]
    );
    let revoked = log
        .as_array()
        .unwrap()
        .iter()
        .find(|event| s(event, "action") == "agent.app_revoked")
        .unwrap();
    assert_eq!(s(revoked, "target"), "Claude Code");
    assert!(revoked["actorId"].is_string());
    assert_eq!(
        revoked["changes"],
        json!([{"field":"reason","from":null,"to":"replaced"}])
    );
}

#[tokio::test]
async fn a_name_a_key_or_another_app_holds_stays_taken() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50003/callback";
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Laptop agent"))
        .await;
    let key = server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/agents/keys",
            json!({"name":"Desk key","allDsps":true,"dsps":[],"access":"read",
                "reads":{"areas":["routes"],"bypass":false},"dspReads":[],"expiresAt":null}),
        )
        .await;
    assert_eq!(key.status, 200, "{}", key.body);
    for (client, redirect, name) in [
        (CHATGPT, CHATGPT_REDIRECT, "laptop agent"),
        (CLAUDE_CODE, local, "Desk key"),
    ] {
        let request = server.requested(client, redirect).await;
        let refused = server
            .as_owner(
                &owner,
                "POST",
                &format!("/api/platform/oauth/requests/{request}/approve"),
                everything(name),
            )
            .await;
        assert_eq!(
            (refused.status, s(&refused.body, "error")),
            (409, "agent_key_name_taken"),
            "{client} {name}"
        );
    }
    // Nothing was replaced.
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(
        listed.body["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|key| key["revokedAt"].is_null())
    );
}

/// Registers an app as it would itself, answering its new client id.
async fn registered(server: &Server, name: &str, redirect: &str) -> String {
    let answer = server
        .send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(json!({"client_name":name,"redirect_uris":[redirect]}).to_string()),
        )
        .await;
    assert_eq!(answer.status, 201, "{}", answer.body);
    s(&answer.body, "client_id").to_owned()
}
/// The connected apps still in use, by name and client.
async fn live_apps(server: &Server, owner: &Owner) -> Vec<(String, String)> {
    let listed = server
        .as_owner(owner, "GET", "/api/platform/agents", json!({}))
        .await;
    listed.body["keys"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|key| key["kind"] == "app" && key["revokedAt"].is_null())
        .map(|key| {
            (
                s(key, "name").to_owned(),
                s(&key["client"], "name").to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn an_app_that_registers_again_replaces_its_earlier_connection() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let first = registered(&server, "Cursor", cursor).await;
    let tokens = server
        .connect(&owner, &first, cursor, everything("Cursor"))
        .await;
    // Each registration is a new client; the same name makes it the same app.
    let second = registered(&server, " cursor ", cursor).await;
    assert_ne!(first, second);
    let again = server
        .connect(&owner, &second, cursor, everything("Cursor"))
        .await;
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&tokens, "access_token"))
            .await
            .status,
        401
    );
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&again, "access_token"))
            .await
            .status,
        200
    );
    assert_eq!(
        live_apps(&server, &owner).await,
        [("Cursor".to_owned(), "cursor".to_owned())]
    );
    // Another self-registered app keeps the name taken.
    let other = registered(&server, "Windsurf", cursor).await;
    let request = server.requested(&other, cursor).await;
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Cursor"),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (409, "agent_key_name_taken")
    );
}

#[tokio::test]
async fn an_unrecognized_app_never_replaces_a_known_one() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50004/callback";
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    // It says it is Claude Code, but only a known app's own client replaces Claude Code.
    let pretender = registered(&server, "Claude Code", local).await;
    let request = server.requested(&pretender, local).await;
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Claude Code"),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (409, "agent_key_name_taken")
    );
    // Under a name of its own it connects beside the known one.
    let request = server.requested(&pretender, local).await;
    let code = server
        .approved(&owner, &request, everything("Claude Code (other)"))
        .await;
    assert_eq!(server.exchange(&pretender, local, &code).await.status, 200);
    let mut live = live_apps(&server, &owner).await;
    live.sort();
    assert_eq!(
        live,
        [
            ("Claude Code".to_owned(), "Claude Code".to_owned()),
            ("Claude Code (other)".to_owned(), "Claude Code".to_owned()),
        ]
    );
    // Nor does a known app replace an unrecognized one that took a name.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Claude Code (other)"),
        )
        .await;
    assert_eq!(s(&refused.body, "error"), "agent_key_name_taken");
}

#[tokio::test]
async fn registrations_never_used_are_capped_across_every_address() {
    let server = Server::paired().await;
    server
        .state
        .run(|db| {
            for index in 0..200 {
                db.platform.exec(
                    "INSERT INTO oauth_clients(id,name,redirect_uris,created_at) \
                     VALUES (?,'App','[]',?)",
                    [format!("dcr_{index:032}"), db::iso()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let refused = server
        .send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(
                    json!({"client_name":"Cursor","redirect_uris":["cursor://a/cb"]}).to_string(),
                ),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (429, "too_many_registrations")
    );
    // An unused registration belongs only to the short pairing attempt; stale rows are
    // removed before capacity is counted.
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_clients SET created_at=? WHERE last_used_at IS NULL",
                [db::at(db::now() - 16 * 60 * 1000)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        registered(&server, "Cursor", "cursor://a/cb").await.len(),
        36
    );
}

#[tokio::test]
async fn registration_is_closed_until_the_owner_opens_pairing() {
    let server = Server::start().await;
    let answer = server
        .send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(
                    json!({"client_name":"Cursor","redirect_uris":["cursor://a/cb"]}).to_string(),
                ),
        )
        .await;
    assert_eq!(
        (answer.status, s(&answer.body, "error")),
        (403, "registration_closed")
    );
    assert_eq!(
        server
            .state
            .read(|db| db.platform.count("SELECT count(*) FROM oauth_clients", []))
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn the_approval_page_says_what_approving_would_replace() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50005/callback";
    let fresh = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{fresh}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(shown.body["replaces"], Value::Null);
    server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let connected_at = {
        let listed = server
            .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
            .await;
        s(&listed.body["keys"][0], "createdAt").to_owned()
    };
    // Under the app's own name, approving replaces that connection; under another, nothing.
    let request = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{request}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(
        shown.body["replaces"],
        json!({"name":"Claude Code","connectedAt":connected_at})
    );
    for (name, replaces) in [
        (
            "claude%20code",
            json!({"name":"Claude Code","connectedAt":connected_at}),
        ),
        ("Laptop", Value::Null),
        ("", json!({"name":"Claude Code","connectedAt":connected_at})),
    ] {
        let asked = server
            .as_owner(&owner, "GET", &format!("{path}?name={name}"), json!({}))
            .await;
        assert_eq!(asked.status, 200, "{}", asked.body);
        assert_eq!(asked.body["replaces"], replaces, "{name}");
    }
    // Another app under that name replaces nothing; approving it is refused instead.
    let other = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{other}?name=Claude%20Code"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["replaces"], Value::Null);
}

#[tokio::test]
async fn authorization_requests_are_counted_by_address_before_any_app_is_looked_up() {
    let server = Server::paired().await;
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(CLAUDE_CODE, "http://localhost:1/callback", &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    for _ in 0..30 {
        let answer = server.authorize(&pairs).await;
        assert!(answer.header("location").contains("#authorize?request="));
    }
    let limited = format!("{}/#authorize?error=rate_limited", server.origin);
    assert_eq!(server.authorize(&pairs).await.header("location"), limited);
    // Whatever the request, unknown app or none.
    let unknown = server
        .authorize(&[("client_id", "https://evil.example/client.json")])
        .await;
    assert_eq!(unknown.header("location"), limited);
}

#[tokio::test]
async fn the_durable_authorization_quota_still_allows_sixty_requests() {
    let server = Server::start().await;
    for _ in 0..60 {
        let redirect = server
            .state
            .run(|db| db.throttle_authorize("198.51.100.42"))
            .await
            .unwrap();
        assert_eq!(redirect, None);
    }
    let redirect = server
        .state
        .run(|db| db.throttle_authorize("198.51.100.42"))
        .await
        .unwrap();
    assert_eq!(
        redirect,
        Some(format!("{}/#authorize?error=rate_limited", server.origin))
    );
}

#[tokio::test]
async fn a_replay_with_the_wrong_verifier_ends_nothing() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server
        .approved(&owner, &request, everything("ChatGPT"))
        .await;
    let tokens = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let replay = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", CHATGPT_REDIRECT),
                ("code_verifier", &"y".repeat(43)),
                ("client_id", CHATGPT),
                ("resource", &server.resource()),
            ],
        )
        .await;
    assert_eq!(s(&replay.body, "error"), "invalid_grant");
    let access = s(&tokens.body, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
}

#[tokio::test]
async fn the_owner_can_deny_and_the_app_is_told() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["app"]["redirectHost"], "chatgpt.com");
    let denied = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/deny"),
            json!({}),
        )
        .await;
    assert_eq!(denied.status, 200, "{}", denied.body);
    assert_eq!(
        s(&denied.body, "redirect"),
        format!(
            "{CHATGPT_REDIRECT}?error=access_denied&state=st%2Fate+1&iss={}",
            url::form_urlencoded::byte_serialize(server.origin.as_bytes()).collect::<String>()
        )
    );
}

#[tokio::test]
async fn refresh_tokens_are_single_use_and_replay_ends_the_family() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://127.0.0.1:61001/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let refresh = s(&first, "refresh_token");
    // Only by the client it was issued to, and only as a refresh token.
    let wrong = server.refresh(CHATGPT, refresh).await;
    assert_eq!(s(&wrong.body, "error"), "invalid_grant");
    let access = server.refresh(CLAUDE_CODE, s(&first, "access_token")).await;
    assert_eq!(s(&access.body, "error"), "invalid_grant");
    let unknown = server.refresh("https://evil.example/c.json", refresh).await;
    assert_eq!(
        (unknown.status, s(&unknown.body, "error")),
        (401, "invalid_client")
    );
    let target = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", CLAUDE_CODE),
                ("resource", &server.origin),
            ],
        )
        .await;
    assert_eq!(s(&target.body, "error"), "invalid_target");
    // Rotated: a new pair, and the old access token still works until it expires.
    let second = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(second.status, 200, "{}", second.body);
    assert_ne!(s(&second.body, "refresh_token"), refresh);
    assert_eq!(second.body["scope"], "dispatch");
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second.body, "access_token"))
            .await
            .status,
        200
    );
    // Reuse is a compromise signal even immediately after rotation: it revokes the connected
    // app and the successor pair instead of minting an independent branch.
    let replay = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(s(&replay.body, "error"), "invalid_grant");
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&second.body, "access_token"))
            .await
            .status,
        401
    );
    let gone = server
        .refresh(CLAUDE_CODE, s(&second.body, "refresh_token"))
        .await;
    assert_eq!(s(&gone.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn simultaneous_refreshes_mint_once_and_then_end_the_compromised_family() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://127.0.0.1:61002/callback";
    let first = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let refresh = s(&first, "refresh_token").to_owned();

    let (left, right) = tokio::join!(
        server.refresh(CLAUDE_CODE, &refresh),
        server.refresh(CLAUDE_CODE, &refresh),
    );
    let mut answers = [left, right];
    answers.sort_by_key(|answer| answer.status);
    assert_eq!(answers[0].status, 200, "{}", answers[0].body);
    assert_eq!(
        (answers[1].status, s(&answers[1].body, "error")),
        (400, "invalid_grant")
    );

    // The losing replay is a compromise signal. Even the pair the winning request briefly
    // minted is unusable by the time both calls return.
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&answers[0].body, "access_token"))
            .await
            .status,
        401
    );
    let successor = server
        .refresh(CLAUDE_CODE, s(&answers[0].body, "refresh_token"))
        .await;
    assert_eq!(s(&successor.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn expired_or_revoked_access_ends_with_invalid_token() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50000/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let access = s(&tokens, "access_token").to_owned();
    // Never as an API key, and never a refresh token as a bearer.
    let keyed = server
        .send(
            server
                .client
                .get(server.url("/api/v1/whoami"))
                .header("x-api-key", &access),
        )
        .await;
    assert_eq!(keyed.status, 401);
    let refresh = server
        .bearer("/api/v1/whoami", s(&tokens, "refresh_token"))
        .await;
    assert_eq!(refresh.status, 401);
    let hash = crypto::sha(&access);
    server
        .state
        .run(move |db| {
            db.platform.exec(
                "UPDATE oauth_tokens SET expires_at=? WHERE hash=?",
                [db::at(db::now() - 1000), hash],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let expired = server.bearer("/api/v1/mcp", &access).await;
    assert_eq!(
        (expired.status, s(&expired.body, "error")),
        (401, "access_token_expired")
    );
    assert!(
        expired
            .header("www-authenticate")
            .ends_with(", error=\"invalid_token\"")
    );
    // A fresh pair works until the owner revokes the app on the Agents page.
    let fresh = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    let access = s(&fresh.body, "access_token").to_owned();
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let id = s(&listed.body["keys"][0], "id").to_owned();
    let edited = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/agents/keys/{id}"),
            json!({"name":"Claude Code","allDsps":true,"dsps":[],"access":"operator",
                "reads":{"areas":EVERY,"bypass":false},"dspReads":[],"expiresAt":null}),
        )
        .await;
    // An app is edited like a key, but only ever reads.
    assert_eq!(
        (edited.status, s(&edited.body, "error")),
        (400, "invalid_input")
    );
    let revoked = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/agents/keys/{id}/revoke"),
            json!({}),
        )
        .await;
    assert_eq!(revoked.status, 200, "{}", revoked.body);
    let answer = server.bearer("/api/v1/mcp", &access).await;
    assert_eq!(answer.status, 401);
    assert!(
        answer
            .header("www-authenticate")
            .ends_with(", error=\"invalid_token\"")
    );
    let after = server
        .refresh(CLAUDE_CODE, s(&fresh.body, "refresh_token"))
        .await;
    assert_eq!(s(&after.body, "error"), "invalid_grant");
}

#[tokio::test]
async fn a_connected_app_reaches_only_what_the_owner_chose() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let tokens = server
        .connect(
            &owner,
            CHATGPT,
            CHATGPT_REDIRECT,
            json!({"name":"ChatGPT","allDsps":false,"dsps":[north],
                "reads":{"areas":["dvic","routes","dvic"],"bypass":false}}),
        )
        .await;
    let access = s(&tokens, "access_token");
    let whoami = server.bearer("/api/v1/whoami", access).await;
    assert_eq!(whoami.status, 200, "{}", whoami.body);
    assert_eq!(
        whoami.body["key"],
        json!({"name":"ChatGPT","access":"read","expiresAt":null})
    );
    let dsps: Vec<&str> = whoami.body["dsps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dsp| s(dsp, "id"))
        .collect();
    assert_eq!(dsps, [north.as_str()]);
    // What the owner approved it to read, each kind once in its order.
    let reads = json!({"areas":["routes","dvic"],"bypass":false});
    assert_eq!(whoami.body["dsps"][0]["reads"], reads);
    // MCP offers the tools that read nothing in particular, and those of what it reads.
    let listed = server.mcp(access, "tools/list").await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let tools: Vec<&str> = listed.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| s(tool, "name"))
        .collect();
    let read: Vec<&str> = dispatch_core::mcp::data::catalog::ENDPOINTS
        .iter()
        .filter(|endpoint| {
            endpoint
                .area
                .is_none_or(|area| [kind("routes"), kind("dvic")].contains(&area))
        })
        .map(|endpoint| endpoint.tool)
        .collect();
    assert_eq!(tools[0], "get_profile");
    assert_eq!(&tools[1..], read);
    assert!(tools.contains(&"route_day") && !tools.contains(&"timecards"));
    // Its calls count like a key's, under the connected app.
    let used = server.state.agents.last();
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let app = &listed.body["keys"][0];
    assert!(used.contains_key(s(app, "id")));
    assert!(app["lastUsedAt"].is_string());
    assert_eq!(app["dsps"], json!([north]));
    assert_eq!((&app["reads"], &app["dspReads"]), (&reads, &json!([])));
}

#[tokio::test]
async fn apps_register_themselves_only_to_come_back_to_this_computer() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let register = |body: Value| {
        server.send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(body.to_string()),
        )
    };
    let hermes = register(
        json!({"response_types":["code"],"client_name":"Hermes Agent",
        "redirect_uris":["http://127.0.0.1:27890/callback"],"token_endpoint_auth_method":"none",
        "grant_types":["authorization_code","refresh_token"],"application_type":"native",
        "scope":"dispatch"}),
    )
    .await;
    assert_eq!(hermes.status, 201, "{}", hermes.body);
    assert_eq!(hermes.header("cache-control"), "no-store");
    let id = s(&hermes.body, "client_id").to_owned();
    assert!(id.starts_with("dcr_"));
    assert_eq!(
        hermes.body["redirect_uris"],
        json!(["http://127.0.0.1:27890/callback"])
    );
    assert_eq!(hermes.body["token_endpoint_auth_method"], "none");
    for (body, error) in [
        (
            json!({"redirect_uris":["https://evil.example/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["http://evil.example/cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["javascript:alert(1)"]}),
            "invalid_redirect_uri",
        ),
        // A website's protocol handler, or a scheme no native app is known by.
        (
            json!({"redirect_uris":["web+dsp://cb"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["magnet:?xt=urn:btih:abc"]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["myapp://cb"]}),
            "invalid_redirect_uri",
        ),
        // At most five redirects, of at most 512 characters each.
        (
            json!({"redirect_uris":(0..6).map(|port| format!("http://localhost:{}/cb", 1000 + port))
                .collect::<Vec<_>>()}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":[format!("http://localhost/{}", "a".repeat(512))]}),
            "invalid_redirect_uri",
        ),
        (
            json!({"redirect_uris":["https://127.0.0.1/cb"]}),
            "invalid_redirect_uri",
        ),
        (json!({"redirect_uris":[]}), "invalid_redirect_uri"),
        (json!({"client_name":"No redirect"}), "invalid_redirect_uri"),
        (
            json!({"redirect_uris":["http://localhost/cb"],"token_endpoint_auth_method":"client_secret_post"}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris":["http://localhost/cb"],"grant_types":["client_credentials"]}),
            "invalid_client_metadata",
        ),
        (
            json!({"redirect_uris":["http://localhost/cb"],"response_types":["token"]}),
            "invalid_client_metadata",
        ),
        (json!(["not an object"]), "invalid_client_metadata"),
    ] {
        let answer = register(body.clone()).await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (400, error),
            "{body}"
        );
    }
    let reverse =
        register(json!({"client_name":"Example","redirect_uris":["com.example.app:/cb"]})).await;
    assert_eq!(reverse.status, 201, "{}", reverse.body);
    // Its own URI scheme opens an app on this computer as well; it matches exactly.
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let registered = register(json!({"client_name":"Cursor","redirect_uris":[cursor]})).await;
    assert_eq!(registered.status, 201, "{}", registered.body);
    let cursor_id = s(&registered.body, "client_id").to_owned();
    let request = server.requested(&cursor_id, cursor).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(
        shown.body["app"],
        json!({"name":"Cursor","clientId":cursor_id,"known":false,
            "redirectHost":"this computer","redirectScheme":"cursor"})
    );
    let code = server
        .approved(&owner, &request, everything("Cursor"))
        .await;
    let tokens = server.exchange(&cursor_id, cursor, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(
        &cursor_id,
        "cursor://anysphere.cursor-retrieval:1/oauth/callback",
        &challenge,
    );
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let answer = server.authorize(&pairs).await;
    assert_eq!(
        answer.header("location"),
        format!("{}/#authorize?error=invalid_redirect", server.origin)
    );
    // A loopback registration takes any port; it is shown as app-provided, by its own name.
    let request = server
        .requested(&id, "http://127.0.0.1:27891/callback")
        .await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["app"]["known"], false);
    assert_eq!(shown.body["app"]["name"], "Hermes Agent");
    // Registering is rate-limited by address.
    let mut last = 0;
    for _ in 0..20 {
        last = register(json!({"redirect_uris":["http://localhost/cb"]}))
            .await
            .status;
    }
    assert_eq!(last, 429);
}

#[tokio::test]
async fn revoking_any_token_ends_the_app_and_unknown_tokens_are_fine() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50001/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    for token in ["dsr_dev_unknown", "anything"] {
        let answer = server
            .form(
                "/oauth/revoke",
                &[("token", token), ("client_id", CLAUDE_CODE)],
            )
            .await;
        assert_eq!(answer.status, 200, "{}", answer.body);
    }
    // A token named by another client is left alone.
    let other = server
        .form(
            "/oauth/revoke",
            &[
                ("token", s(&tokens, "refresh_token")),
                ("client_id", CHATGPT),
            ],
        )
        .await;
    assert_eq!(other.status, 200);
    let access = s(&tokens, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
    // Claude Code signs out with its refresh token, then its access token.
    for token in [s(&tokens, "refresh_token"), access] {
        let answer = server
            .form(
                "/oauth/revoke",
                &[
                    ("token", token),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", CLAUDE_CODE),
                ],
            )
            .await;
        assert_eq!(answer.status, 200);
    }
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 401);
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert!(listed.body["keys"][0]["revokedAt"].is_string());
    // Form posts only, as RFC 6749 says, and never 415.
    let json_body = server
        .send(
            server
                .client
                .post(server.url("/oauth/token"))
                .header("content-type", "application/json")
                .body("{}"),
        )
        .await;
    assert_eq!(
        (json_body.status, s(&json_body.body, "error")),
        (400, "invalid_request")
    );
    let grant = server
        .form(
            "/oauth/token",
            &[
                ("grant_type", "password"),
                ("client_id", CLAUDE_CODE),
                ("resource", &server.resource()),
            ],
        )
        .await;
    assert_eq!(s(&grant.body, "error"), "unsupported_grant_type");
}

#[tokio::test]
async fn token_ingress_rejects_cross_site_browsers_and_stops_before_database_saturation() {
    let server = Server::start().await;
    let resource = server.resource();
    let cross_site = server
        .send(
            server
                .client
                .post(server.url("/oauth/token"))
                .header("origin", "https://evil.example")
                .header("sec-fetch-site", "cross-site")
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", "not-a-token"),
                    ("client_id", CLAUDE_CODE),
                    ("resource", resource.as_str()),
                ]),
        )
        .await;
    assert_eq!(
        (cross_site.status, s(&cross_site.body, "error")),
        (400, "invalid_request")
    );

    let fields = [
        ("grant_type", "refresh_token"),
        ("refresh_token", "not-a-token"),
        ("client_id", CLAUDE_CODE),
        ("resource", resource.as_str()),
    ];
    for _ in 0..120 {
        assert_eq!(server.form("/oauth/token", &fields).await.status, 400);
    }
    let limited = server.form("/oauth/token", &fields).await;
    assert_eq!(
        (limited.status, s(&limited.body, "error")),
        (429, "rate_limited")
    );
    assert_eq!(limited.header("retry-after"), "60");
    // The process remains responsive and ordinary database reads are not queued behind more
    // invalid token work once the pre-database budget is spent.
    assert_eq!(server.get("/api/health").await.status, 200);
}

#[tokio::test]
async fn registration_rejects_cross_site_browsers_before_spending_its_budget() {
    let server = Server::paired().await;
    for _ in 0..20 {
        let refused = server
            .send(
                server
                    .client
                    .post(server.url("/oauth/register"))
                    .header("origin", "https://evil.example")
                    .header("sec-fetch-site", "cross-site")
                    .form(&[("client_name", "Browser")]),
            )
            .await;
        assert_eq!(
            (refused.status, s(&refused.body, "error")),
            (400, "invalid_request")
        );
    }

    // Cross-site refusals consumed neither the process budget nor registration capacity. The
    // same source can still use its complete durable allowance for real JSON registrations.
    for index in 0..20 {
        let answer = server
            .send(
                server
                    .client
                    .post(server.url("/oauth/register"))
                    .header("content-type", "application/json")
                    .body(
                        json!({
                            "client_name":format!("App {index}"),
                            "redirect_uris":[format!("com.example.app{index}:/callback")]
                        })
                        .to_string(),
                    ),
            )
            .await;
        assert_eq!(answer.status, 201, "{index}: {}", answer.body);
    }
    let limited = server
        .send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(
                    json!({"client_name":"One too many","redirect_uris":["com.example.last:/callback"]})
                        .to_string(),
                ),
        )
        .await;
    assert_eq!(
        (limited.status, s(&limited.body, "error")),
        (429, "rate_limited")
    );
}

#[test]
fn what_can_no_longer_be_used_is_pruned() {
    dispatch_backend::install();
    let (_root, db, _) = common::bootstrapped();
    let old = db::at(db::now() - 2 * 24 * 60 * 60 * 1000);
    let soon = db::at(db::now() + 60_000);
    let ancient = db::at(db::now() - 100 * 24 * 60 * 60 * 1000);
    let owner = s(
        &db.platform
            .one("SELECT id FROM users LIMIT 1", [])
            .unwrap()
            .unwrap(),
        "id",
    )
    .to_owned();
    db.platform
        .exec(
            "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,locations,\
             created_at,kind) VALUES ('app','App','app:app','',?,1,'read','full',0,?,'app')",
            [&owner, &old],
        )
        .unwrap();
    for (id, at) in [("expired", &old), ("waiting", &soon)] {
        db.platform
            .exec(
                "INSERT INTO oauth_requests(id,client_id,client_name,verified,redirect_uri,state,\
                 code_challenge,resource,scope,created_at,expires_at) \
                 VALUES (?,'c','C',1,'r',NULL,'x','res','dispatch',?,?)",
                [id, at.as_str(), at.as_str()],
            )
            .unwrap();
        db.platform
            .exec(
                "INSERT INTO oauth_codes(hash,client_id,client_name,client_verified,redirect_uri,\
                 code_challenge,resource,choices,approved_by,created_at,expires_at) \
                 VALUES (?,'c','C',1,'r','x','res','{}',?,?,?)",
                [id, owner.as_str(), at.as_str(), at.as_str()],
            )
            .unwrap();
        db.platform
            .exec(
                "INSERT INTO oauth_tokens(hash,key_id,kind,resource,created_at,expires_at) \
                 VALUES (?,'app','access','res',?,?)",
                [id, at.as_str(), at.as_str()],
            )
            .unwrap();
    }
    for (id, created, used) in [
        ("dcr_never", Some(&old), None),
        ("dcr_new", Some(&soon), None),
        ("dcr_idle", Some(&ancient), Some(&ancient)),
        ("dcr_used", Some(&ancient), Some(&old)),
    ] {
        db.platform
            .exec(
                "INSERT INTO oauth_clients VALUES (?,'App','[]',?,?)",
                rusqlite::params![id, created, used],
            )
            .unwrap();
    }
    // An unused registration is tied to its short pairing attempt.
    db.platform
        .exec(
            "INSERT INTO oauth_clients VALUES ('dcr_today','App','[]',?,NULL)",
            [db::at(db::now() - 12 * 60 * 60 * 1000)],
        )
        .unwrap();
    db.platform
        .exec(
            "UPDATE oauth_clients SET created_at=? WHERE id='dcr_never'",
            [db::at(db::now() - 25 * 60 * 60 * 1000)],
        )
        .unwrap();
    db.prune_oauth().unwrap();
    let left = |table: &str, column: &str| -> Vec<String> {
        db.platform
            .all(&format!("SELECT {column} FROM {table} ORDER BY 1"), [])
            .unwrap()
            .iter()
            .map(|row| s(row, column).to_owned())
            .collect()
    };
    assert_eq!(left("oauth_requests", "id"), ["waiting"]);
    assert_eq!(left("oauth_codes", "hash"), ["waiting"]);
    assert_eq!(left("oauth_tokens", "hash"), ["waiting"]);
    assert_eq!(left("oauth_clients", "id"), ["dcr_new", "dcr_used"]);
}

/// Ends the pairing window, as ten minutes passing would.
async fn close_pairing(server: &Server) {
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_pairing SET open_until=?",
                [db::at(db::now() - 1000)],
            )?;
            Ok(())
        })
        .await
        .unwrap();
}
/// The location an authorization request is sent to, for `client` at `redirect`.
async fn asked(server: &Server, client: &str, redirect: &str) -> String {
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(client, redirect, &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let answer = server.authorize(&pairs).await;
    assert_eq!(answer.status, 302, "{}", answer.body);
    answer.header("location").to_owned()
}
/// The owner's audited actions with this prefix, oldest first.
async fn logged(server: &Server, prefix: &'static str) -> Vec<(String, String)> {
    let log = server
        .state
        .read(|db| common::audits(db, None))
        .await
        .unwrap();
    let mut actions: Vec<(String, String)> = log
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| s(event, "action").starts_with(prefix))
        .map(|event| (s(event, "action").to_owned(), s(event, "target").to_owned()))
        .collect();
    actions.reverse();
    actions
}
const DAY: i64 = 24 * 60 * 60 * 1000;

#[tokio::test]
async fn apps_ask_to_connect_only_while_the_pairing_window_is_open() {
    let server = Server::start().await;
    let owner = server.owner().await;
    let local = "http://localhost:50010/callback";
    let closed = format!("{}/#authorize?error=pairing_closed", server.origin);
    let pairing = "/api/platform/oauth/pairing";
    let requests = || {
        server
            .state
            .read(|db| db.platform.count("SELECT count(*) FROM oauth_requests", []))
    };
    // Closed until the owner opens it: the browser goes to Dispatch's page, never to the
    // app, and nothing is stored.
    let shown = server.as_owner(&owner, "GET", pairing, json!({})).await;
    assert_eq!(
        (shown.status, &shown.body),
        (200, &json!({"openUntil":null}))
    );
    assert_eq!(asked(&server, CLAUDE_CODE, local).await, closed);
    assert_eq!(requests().await.unwrap(), 0);
    // Only a platform owner opens it, and with no fresh verification: a session signed in a
    // day ago does.
    assert_eq!(server.get(pairing).await.status, 401);
    let earlier = server.signed_in(db::now() - DAY).await;
    let opened = server.as_owner(&earlier, "POST", pairing, json!({})).await;
    assert_eq!(opened.status, 200, "{}", opened.body);
    let until = s(&opened.body, "openUntil").to_owned();
    let left = chrono::DateTime::parse_from_rfc3339(&until)
        .unwrap()
        .timestamp_millis()
        - db::now();
    assert!(left > 9 * 60_000 && left <= 10 * 60_000, "{until}");
    let shown = server.as_owner(&owner, "GET", pairing, json!({})).await;
    assert_eq!(shown.body, json!({"openUntil":until}));
    let refused = server
        .as_owner(&owner, "POST", pairing, json!({"minutes":60}))
        .await;
    assert_eq!(refused.status, 400);
    // Open, an app asks and its request waits for the owner.
    let request = server.requested(CLAUDE_CODE, local).await;
    assert_eq!(requests().await.unwrap(), 1);
    // Opening it again keeps it open ten minutes from then; the log has the one opening.
    let again = server.as_owner(&owner, "POST", pairing, json!({})).await;
    let extended = s(&again.body, "openUntil").to_owned();
    assert!(extended >= until, "{extended} {until}");
    assert_eq!(
        logged(&server, "agent.pairing").await,
        [("agent.pairing_opened".to_owned(), String::new())]
    );
    // It is stored, so a restart keeps it open.
    let restarted = State::new(server.state.config.clone()).unwrap();
    let kept = restarted.read(|db| db.oauth_pairing()).await.unwrap();
    assert_eq!(kept.open_until, Some(extended));
    // Closed again, nothing more may ask; but the request made while it was open is still
    // approved, and the app signs in, renews and signs out without the window.
    close_pairing(&server).await;
    assert_eq!(asked(&server, CLAUDE_CODE, local).await, closed);
    assert_eq!(requests().await.unwrap(), 1);
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let renewed = server
        .refresh(CLAUDE_CODE, s(&tokens.body, "refresh_token"))
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    let access = s(&renewed.body, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
    let signed_out = server
        .form(
            "/oauth/revoke",
            &[("token", access), ("client_id", CLAUDE_CODE)],
        )
        .await;
    assert_eq!(signed_out.status, 200);
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 401);
    // Reopened after closing, the opening is logged again.
    server.as_owner(&owner, "POST", pairing, json!({})).await;
    assert_eq!(logged(&server, "agent.pairing").await.len(), 2);
}

#[tokio::test]
async fn the_owner_chooses_which_kinds_of_app_may_connect() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let path = "/api/platform/oauth/apps";
    let listed = server.as_owner(&owner, "GET", path, json!({})).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let defaults = json!({"apps":[
        {"id":"chatgpt","name":"ChatGPT","allowed":true},
        {"id":"codex","name":"Codex","allowed":true},
        {"id":"claude-code","name":"Claude Code","allowed":true},
        {"id":"hermes","name":"Hermes Agent","allowed":true},
        {"id":"local","name":"Apps on this computer","allowed":true},
        {"id":"web","name":"Websites and other apps","allowed":false},
    ]});
    assert_eq!(listed.body, defaults);
    assert_eq!(server.get(path).await.status, 401);
    // Choosing is routine, as approving is: a session signed in a day ago chooses below.
    let earlier = server.signed_in(db::now() - DAY).await;
    for body in [
        json!({"id":"cursor","allowed":false}),
        json!({"id":"codex"}),
        json!({"id":"codex","allowed":"no"}),
        json!({"id":"codex","allowed":false,"until":1}),
    ] {
        let refused = server.as_owner(&owner, "POST", path, body.clone()).await;
        assert_eq!(
            (refused.status, s(&refused.body, "error")),
            (400, "invalid_input"),
            "{body}"
        );
    }
    assert_eq!(
        server.as_owner(&owner, "GET", path, json!({})).await.body,
        defaults
    );
    // An app connected before its kind is turned off stays connected, and renews.
    let local = "http://localhost:50011/callback";
    let tokens = server
        .connect(&owner, CLAUDE_CODE, local, everything("Claude Code"))
        .await;
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let registered_app = registered(&server, "Cursor", cursor).await;
    for id in ["claude-code", "local"] {
        let changed = server
            .as_owner(&earlier, "POST", path, json!({"id":id,"allowed":false}))
            .await;
        assert_eq!(changed.status, 200, "{}", changed.body);
        let app = changed.body["apps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|app| app["id"] == id)
            .unwrap()
            .clone();
        assert_eq!(app["allowed"], false);
    }
    // Turned off, an app is refused on Dispatch's page, which names it; the others ask.
    let page = |app: &str| {
        format!(
            "{}/#authorize?error=app_not_allowed&app={app}",
            server.origin
        )
    };
    assert_eq!(
        asked(&server, CLAUDE_CODE, local).await,
        page("claude-code")
    );
    assert_eq!(asked(&server, &registered_app, cursor).await, page("local"));
    server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let renewed = server
        .refresh(CLAUDE_CODE, s(&tokens, "refresh_token"))
        .await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    // Turned on again, it asks again. Choosing what is already chosen changes nothing.
    for _ in 0..2 {
        let changed = server
            .as_owner(
                &owner,
                "POST",
                path,
                json!({"id":"claude-code","allowed":true}),
            )
            .await;
        assert_eq!(changed.status, 200, "{}", changed.body);
    }
    server.requested(CLAUDE_CODE, local).await;
    let changes: Vec<(String, String)> = logged(&server, "agent.app_")
        .await
        .into_iter()
        .filter(|(action, _)| action.ends_with("allowed"))
        .collect();
    assert_eq!(
        changes,
        [
            ("agent.app_disallowed".to_owned(), "Claude Code".to_owned()),
            (
                "agent.app_disallowed".to_owned(),
                "Apps on this computer".to_owned()
            ),
            ("agent.app_allowed".to_owned(), "Claude Code".to_owned()),
        ]
    );
}

/// The internet as a test sees it: each host's addresses and each URL's answer, with every
/// host looked up and every address connected to.
#[derive(Default)]
struct Internet {
    hosts: std::collections::HashMap<&'static str, Vec<std::net::IpAddr>>,
    pages: std::collections::HashMap<String, (u16, Vec<u8>)>,
    resolved: std::sync::Mutex<Vec<String>>,
    connected: std::sync::Mutex<Vec<(String, std::net::SocketAddr)>>,
}
impl Network for Internet {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
    ) -> Pending<'a, dispatch_core::Result<Vec<std::net::IpAddr>>> {
        Box::pin(async move {
            self.resolved.lock().unwrap().push(host.to_owned());
            self.hosts
                .get(host)
                .cloned()
                .ok_or_else(|| dispatch_core::Error::new("app_unavailable", 502))
        })
    }
    fn get<'a>(
        &'a self,
        url: &'a url::Url,
        address: std::net::SocketAddr,
        limit: usize,
    ) -> Pending<'a, dispatch_core::Result<(u16, Vec<u8>)>> {
        Box::pin(async move {
            self.connected
                .lock()
                .unwrap()
                .push((url.to_string(), address));
            let (status, mut body) = self
                .pages
                .get(url.as_str())
                .cloned()
                .unwrap_or((404, Vec::new()));
            body.truncate(limit + 1);
            Ok((status, body))
        })
    }
}
fn client_document(client_id: &str, name: &str, redirect: &str) -> Vec<u8> {
    json!({"client_id":client_id,"client_name":name,"redirect_uris":[redirect]})
        .to_string()
        .into_bytes()
}

#[tokio::test]
async fn websites_connect_only_when_allowed_and_only_from_public_addresses() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let site = "https://tools.example.com/oauth/client.json";
    let callback = "https://tools.example.com/callback";
    let v4 = std::net::IpAddr::from;
    let v6 = |address: &str| -> std::net::IpAddr { address.parse().unwrap() };
    let public = v4([93, 184, 215, 14]);
    let mut internet = Internet::default();
    internet.hosts.insert("tools.example.com", vec![public]);
    internet.pages.insert(
        site.into(),
        (200, client_document(site, "Example Tools", callback)),
    );
    let refused = [
        ("private.example.com", vec![v4([10, 1, 2, 3])]),
        ("loopback.example.com", vec![v4([127, 0, 0, 1])]),
        ("metadata.example.com", vec![v4([169, 254, 169, 254])]),
        ("shared.example.com", vec![v4([100, 64, 0, 9])]),
        ("documentation.example.com", vec![v4([192, 0, 2, 10])]),
        ("multicast.example.com", vec![v4([239, 1, 2, 3])]),
        ("loopback6.example.com", vec![v6("::1")]),
        ("unique6.example.com", vec![v6("fd00:ec2::254")]),
        ("linklocal6.example.com", vec![v6("fe80::1")]),
        ("mapped6.example.com", vec![v6("::ffff:a00:1")]),
        ("mixed.example.com", vec![public, v4([10, 0, 0, 1])]),
        ("nowhere.example.com", vec![]),
    ];
    for (host, addresses) in &refused {
        internet.hosts.insert(host, addresses.clone());
        let url = format!("https://{host}/client.json");
        let document = client_document(&url, "Sneaky", &format!("https://{host}/cb"));
        internet.pages.insert(url, (200, document));
    }
    for host in [
        "moved.example.com",
        "large.example.com",
        "other.example.com",
    ] {
        internet.hosts.insert(host, vec![public]);
    }
    internet.pages.insert(
        "https://moved.example.com/client.json".into(),
        (302, Vec::new()),
    );
    let mut large = client_document(
        "https://large.example.com/client.json",
        "Large",
        "https://large.example.com/cb",
    );
    large.resize(64 * 1024 + 1, b' ');
    internet
        .pages
        .insert("https://large.example.com/client.json".into(), (200, large));
    internet.pages.insert(
        "https://other.example.com/client.json".into(),
        (200, client_document(site, "Other", callback)),
    );
    let internet = Arc::new(internet);
    server.state.oauth.use_network(internet.clone());
    let page = |error: &str| format!("{}/#authorize?error={error}", server.origin);
    let register = |redirect: &str| {
        server.send(
            server
                .client
                .post(server.url("/oauth/register"))
                .header("content-type", "application/json")
                .body(json!({"client_name":"Web tool","redirect_uris":[redirect]}).to_string()),
        )
    };

    // Off by default: a website's client id is no app Dispatch knows, nothing is fetched for
    // it, and no app registers a website's redirect.
    assert_eq!(asked(&server, site, callback).await, page("unknown_app"));
    assert!(internet.resolved.lock().unwrap().is_empty());
    let dcr = "https://tools.example.com/dcr/callback";
    assert_eq!(
        s(&register(dcr).await.body, "error"),
        "invalid_redirect_uri"
    );
    let allowed = server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"web","allowed":true}),
        )
        .await;
    assert_eq!(allowed.status, 200, "{}", allowed.body);

    // On, its document is read from the address its host has, and from nowhere else; the
    // The owner sees an unrecognized app and where it sends access.
    let request = server.requested(site, callback).await;
    assert_eq!(
        *internet.connected.lock().unwrap(),
        [(site.to_owned(), std::net::SocketAddr::new(public, 443))]
    );
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(
        shown.body["app"],
        json!({"name":"Example Tools","clientId":site,"known":false,
            "redirectHost":"tools.example.com","redirectScheme":null})
    );
    let code = server
        .approved(&owner, &request, everything("Example Tools"))
        .await;
    let tokens = server.exchange(site, callback, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let renewed = server.refresh(site, s(&tokens.body, "refresh_token")).await;
    assert_eq!(renewed.status, 200, "{}", renewed.body);
    let access = s(&renewed.body, "access_token").to_owned();
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
    assert_eq!(
        live_apps(&server, &owner).await,
        [("Example Tools".to_owned(), "Example Tools".to_owned())]
    );
    // Its document is kept for the hour: asking again fetches nothing.
    server.requested(site, callback).await;
    assert_eq!(internet.connected.lock().unwrap().len(), 1);
    // A host with any address that is not public is never connected to; nor is a redirect
    // followed, a document longer than 64 KiB read, or another app's document taken.
    for host in refused.iter().map(|(host, _)| *host).chain([
        "moved.example.com",
        "large.example.com",
        "other.example.com",
    ]) {
        let url = format!("https://{host}/client.json");
        let location = asked(&server, &url, &format!("https://{host}/cb")).await;
        assert_eq!(location, page("app_unavailable"), "{host}");
        assert!(internet.resolved.lock().unwrap().contains(&host.to_owned()));
    }
    let connected: Vec<String> = internet
        .connected
        .lock()
        .unwrap()
        .iter()
        .map(|(url, _)| url.clone())
        .collect();
    assert_eq!(
        connected,
        [
            site,
            "https://moved.example.com/client.json",
            "https://large.example.com/client.json",
            "https://other.example.com/client.json",
        ]
    );
    // An address, plain http, a query or a known app's lookalike is no website's client id,
    // or no known app.
    for client in [
        "https://127.0.0.1/client.json",
        "https://[::1]/client.json",
        "http://tools.example.com/oauth/client.json",
        "https://tools.example.com/oauth/client.json?x=1",
        "https://tools.example.com:8443/oauth/client.json",
    ] {
        assert_eq!(
            asked(&server, client, callback).await,
            page("unknown_app"),
            "{client}"
        );
    }
    // An app registers a website's redirect while websites may connect, and is shown by it.
    let registered_web = register(dcr).await;
    assert_eq!(registered_web.status, 201, "{}", registered_web.body);
    let registered_id = s(&registered_web.body, "client_id").to_owned();
    let request = server.requested(&registered_id, dcr).await;
    let shown = server
        .as_owner(
            &owner,
            "GET",
            &format!("/api/platform/oauth/requests/{request}"),
            json!({}),
        )
        .await;
    assert_eq!(shown.body["app"]["redirectHost"], "tools.example.com");
    assert_eq!(shown.body["app"]["known"], false);
    for redirect in [
        "https://203.0.113.5/cb",
        "https://tools.example.com/cb#x",
        "https://user@tools.dispatch.test/cb",
        "https://Tools.example.com/cb",
    ] {
        assert_eq!(
            s(&register(redirect).await.body, "error"),
            "invalid_redirect_uri",
            "{redirect}"
        );
    }
    // Off again, no website asks; one already connected stays connected.
    server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"web","allowed":false}),
        )
        .await;
    assert_eq!(asked(&server, site, callback).await, page("unknown_app"));
    assert_eq!(
        asked(&server, &registered_id, dcr).await,
        format!("{}&app=web", page("app_not_allowed"))
    );
    assert_eq!(server.bearer("/api/v1/whoami", &access).await.status, 200);
}

#[tokio::test]
async fn in_fixture_mode_the_sample_website_connects_without_the_network() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"web","allowed":true}),
        )
        .await;
    let site = dispatch_core::mcp::oauth::network::FIXTURE_APP;
    let callback = "https://app.dispatch.test/oauth/callback";
    let tokens = server
        .connect(&owner, site, callback, everything("Example web app"))
        .await;
    assert_eq!(
        server
            .bearer("/api/v1/whoami", s(&tokens, "access_token"))
            .await
            .status,
        200
    );
    let unknown = asked(
        &server,
        "https://elsewhere.example.com/client.json",
        callback,
    )
    .await;
    assert_eq!(
        unknown,
        format!("{}/#authorize?error=app_unavailable", server.origin)
    );
}

/// The notices queued for platform owners about connected apps: recipient, subject, text.
async fn notices(server: &Server) -> Vec<(String, String, String)> {
    let key = server.state.key.clone();
    server
        .state
        .read(move |db| {
            Ok(db
                .platform
                .all(
                    "SELECT id,encrypted_message,user_id FROM outbox WHERE kind='connected_app' \
                     ORDER BY created_at,id",
                    [],
                )?
                .iter()
                .map(|row| {
                    let mail =
                        crypto::decrypt(&key, s(row, "id"), s(row, "encrypted_message")).unwrap();
                    (
                        s(&mail, "to").to_owned(),
                        s(&mail, "subject").to_owned(),
                        s(&mail, "text").to_owned(),
                    )
                })
                .collect())
        })
        .await
        .unwrap()
}

/// The texts of the notices with this subject.
fn told<'a>(sent: &'a [(String, String, String)], subject: &str) -> Vec<&'a str> {
    sent.iter()
        .filter(|(_, sent, _)| sent == subject)
        .map(|(.., text)| text.as_str())
        .collect()
}

#[tokio::test]
async fn platform_owners_hear_when_an_app_connects_and_when_dispatch_ends_one() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    // Every active platform owner hears; a member, or an owner no longer active, does not.
    server
        .state
        .run(|db| {
            for (id, email, owner, status) in [
                ("usr_second", "second@dispatch.test", 1, "active"),
                ("usr_gone", "gone@dispatch.test", 1, "disabled"),
            ] {
                db.platform.exec(
                    "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,\
                     status,created_at) VALUES (?,?,'Other','Owner','x',?,?,?)",
                    rusqlite::params![id, email, owner, status, db::iso()],
                )?;
            }
            Ok(())
        })
        .await
        .unwrap();
    let north = server.dsp("Northline Logistics").await;
    let local = "http://localhost:50012/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let code = server
        .approved(
            &owner,
            &request,
            json!({"name":"Laptop","allDsps":false,"dsps":[north],
                "reads":{"areas":["routes","timecards"],"bypass":true}}),
        )
        .await;
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let mut sent = notices(&server).await;
    sent.sort();
    let recipients: Vec<&str> = sent.iter().map(|(to, ..)| to.as_str()).collect();
    assert_eq!(recipients, ["owner@dispatch.test", "second@dispatch.test"]);
    for (to, subject, text) in &sent {
        assert_eq!(subject, "[Dispatch Dev] Claude Code connected to Dispatch");
        for line in [
            "Connection: Laptop",
            "App: Claude Code (known metadata)",
            "Sends access to: this computer",
            "DSPs: Northline Logistics",
            "Access: Reads Routes & packages, Timecards. Bypasses switched-off features.",
            "Approved by: Platform Owner",
            &format!("{}/#agents?tab=apps", server.origin),
            &format!("This notice was sent to {to}"),
        ] {
            assert!(text.contains(line), "{line}\n{text}");
        }
    }
    // Renewing tells nobody; presenting a rotated refresh token again ends the app, and the
    // owners hear why.
    let refresh = s(&tokens.body, "refresh_token").to_owned();
    assert_eq!(server.refresh(CLAUDE_CODE, &refresh).await.status, 200);
    assert_eq!(notices(&server).await.len(), 2);
    let late = server.refresh(CLAUDE_CODE, &refresh).await;
    assert_eq!(s(&late.body, "error"), "invalid_grant");
    let sent = notices(&server).await;
    assert_eq!(sent.len(), 4);
    let ended = told(&sent, "[Dispatch Dev] Dispatch disconnected Claude Code");
    assert_eq!(ended.len(), 2);
    for text in ended {
        assert!(text.contains("already used was presented again"), "{text}");
        assert!(text.contains("Connection: Laptop"), "{text}");
    }
    // A replayed code ends what it made, and they hear that too, once.
    let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let code = server
        .approved(&owner, &request, everything("ChatGPT"))
        .await;
    assert_eq!(
        server
            .exchange(CHATGPT, CHATGPT_REDIRECT, &code)
            .await
            .status,
        200
    );
    // An older release, run again in a rollback, stopped its addresses in the only column it
    // knows: the owners hear it reads everything else.
    server
        .state
        .run(|db| {
            db.platform
                .exec("UPDATE agent_keys SET locations=0 WHERE name='ChatGPT'", [])?;
            Ok(())
        })
        .await
        .unwrap();
    for _ in 0..2 {
        let replay = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
        assert_eq!(s(&replay.body, "error"), "invalid_grant");
    }
    let sent = notices(&server).await;
    assert_eq!(sent.len(), 8);
    let ended = told(&sent, "[Dispatch Dev] Dispatch disconnected ChatGPT");
    assert_eq!(ended.len(), 2);
    for text in ended {
        assert!(text.contains("one-time code"), "{text}");
        assert!(text.contains("Sent access to: chatgpt.com"), "{text}");
        assert!(
            text.contains(
                "Access: Reads Routes & packages, Timecards, Meal breaks, DVIC inspections, \
                 Customer feedback, Safety events, Returns & contact compliance, Weekly \
                 scorecard\n"
            ),
            "{text}"
        );
    }
    // An app signing out, or the owner revoking one, is no news to them.
    let signed_in = server
        .connect(&owner, CLAUDE_CODE, local, everything("Desktop"))
        .await;
    assert_eq!(notices(&server).await.len(), 10);
    server
        .form(
            "/oauth/revoke",
            &[("token", s(&signed_in, "refresh_token"))],
        )
        .await;
    assert_eq!(notices(&server).await.len(), 10);
}

#[test]
fn a_notice_waiting_to_be_sent_goes_only_to_a_platform_owner_still_active() {
    dispatch_backend::install();
    let (_root, db) = common::seeded();
    let user = |email: &str| {
        let (id,): (String,) = db
            .platform
            .one_as("SELECT id FROM users WHERE email=?", [email])
            .unwrap()
            .unwrap();
        id
    };
    let (owner, member) = (user("owner@dispatch.test"), user("member@dispatch.test"));
    db.platform
        .exec(
            "INSERT INTO users(id,email,first_name,last_name,password,platform_owner,status,\
             created_at) VALUES ('usr_gone','gone@dispatch.test','Gone','Owner','x',1,\
             'disabled',?)",
            [db::iso()],
        )
        .unwrap();
    for (id, to) in [
        ("mail_owner", owner.as_str()),
        ("mail_member", member.as_str()),
        ("mail_gone", "usr_gone"),
    ] {
        db.platform
            .exec(
                "INSERT INTO outbox(id,encrypted_message,available_at,created_at,kind,user_id) \
                 VALUES (?1,'',?3,?3,'connected_app',?2)",
                rusqlite::params![id, to, db::now()],
            )
            .unwrap();
    }
    dispatch_core::server::mail::discard_stale(&db).unwrap();
    let (left,): (String,) = db
        .platform
        .one_as("SELECT group_concat(id) FROM outbox", [])
        .unwrap()
        .unwrap();
    assert_eq!(left, "mail_owner");
}

/// The request an authorization's redirect names.
fn request_id(answer: &Answer) -> String {
    let location = answer.header("location");
    location.rsplit_once("request=").unwrap().1.to_owned()
}

#[tokio::test]
async fn only_the_browser_that_brought_a_request_sees_or_answers_it() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50013/callback";
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(CLAUDE_CODE, local, &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let started = server.authorize(&pairs).await;
    let first = request_id(&started);
    // The browser the app sent is given a nonce for the request, in a cookie named for it,
    // for ten minutes.
    let cookie = started.header("set-cookie").to_owned();
    let (value, attributes) = cookie.split_once(';').unwrap();
    let nonce = value
        .strip_prefix(&format!("{}=", browser_cookie(&first)))
        .unwrap()
        .to_owned();
    assert_eq!(nonce.len(), 43);
    assert_eq!(attributes, " Path=/; HttpOnly; SameSite=Lax; Max-Age=600");
    let stored = server
        .state
        .read(|db| {
            db.platform
                .one_as::<(String,)>("SELECT browser FROM oauth_requests", [])
        })
        .await
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(stored, crypto::sha(&nonce));
    let path = |id: &str, suffix: &str| format!("/api/platform/oauth/requests/{id}{suffix}");
    let refused = |answer: Answer| {
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (403, "wrong_browser"),
            "{}",
            answer.body
        );
    };
    let answers = [
        ("GET", "", json!({})),
        ("POST", "/approve", everything("Claude Code")),
        ("POST", "/deny", json!({})),
    ];
    // The owner signed in elsewhere, such as from a link someone sent them, cannot.
    let held = server.browser();
    server.set_browser(&[]);
    for (method, suffix, body) in &answers {
        refused(
            server
                .as_owner(&owner, method, &path(&first, suffix), body.clone())
                .await,
        );
    }
    // Nor can a browser holding only another request's cookie, that request's nonce under
    // this request's name, an empty one, or this one twice.
    let second = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
    let other = server.browser();
    let other_nonce = other[0].split_once('=').unwrap().1.to_owned();
    let name = browser_cookie(&first);
    for cookies in [
        other.clone(),
        vec![format!("{name}={other_nonce}")],
        vec![format!("{name}=")],
        vec![value.to_owned(), value.to_owned()],
    ] {
        server.set_browser(&cookies);
        for (method, suffix, body) in &answers {
            refused(
                server
                    .as_owner(&owner, method, &path(&first, suffix), body.clone())
                    .await,
            );
        }
    }
    // Both requests still wait: refusing the wrong browser changed nothing.
    server.set_browser(&held);
    let shown = server
        .as_owner(&owner, "GET", &path(&first, ""), json!({}))
        .await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    // Answered, its cookie is cleared; the request is gone, and its old cookie finds nothing.
    let approved = server
        .as_owner(
            &owner,
            "POST",
            &path(&first, "/approve"),
            everything("Claude Code"),
        )
        .await;
    assert_eq!(approved.status, 200, "{}", approved.body);
    assert_eq!(
        approved.header("set-cookie"),
        format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    );
    assert!(server.browser().is_empty());
    refused(
        server
            .as_owner(&owner, "GET", &path(&first, ""), json!({}))
            .await,
    );
    server.set_browser(&held);
    let gone = server
        .as_owner(&owner, "GET", &path(&first, ""), json!({}))
        .await;
    assert_eq!(
        (gone.status, s(&gone.body, "error")),
        (404, "authorization_not_found")
    );
    // Denying clears its own cookie too.
    server.set_browser(&other);
    let denied = server
        .as_owner(&owner, "POST", &path(&second, "/deny"), json!({}))
        .await;
    assert_eq!(denied.status, 200, "{}", denied.body);
    assert!(server.browser().is_empty());
    // A refused authorization sets no cookie.
    let closed = server
        .authorize(&[("client_id", "evil"), ("redirect_uri", local)])
        .await;
    assert_eq!(closed.header("set-cookie"), "");
}

#[tokio::test]
async fn apps_started_together_in_one_browser_are_each_approved_in_either_order() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50015/callback";
    for first_answered in [true, false] {
        // The owner copies one app's sign-in command, then another's, before approving either.
        let first = server.requested(CLAUDE_CODE, local).await;
        let second = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
        assert_eq!(server.browser().len(), 2);
        for id in [&first, &second] {
            let shown = server
                .as_owner(
                    &owner,
                    "GET",
                    &format!("/api/platform/oauth/requests/{id}"),
                    json!({}),
                )
                .await;
            assert_eq!(shown.status, 200, "{}", shown.body);
        }
        let order = if first_answered {
            [&first, &second]
        } else {
            [&second, &first]
        };
        // Each approval clears its own cookie and leaves the other one's.
        for (answered, id) in order.into_iter().enumerate() {
            let suffix = if answered == 0 { "a" } else { "b" };
            let name = format!("App {first_answered} {suffix}");
            let approved = server
                .as_owner(
                    &owner,
                    "POST",
                    &format!("/api/platform/oauth/requests/{id}/approve"),
                    everything(&name),
                )
                .await;
            assert_eq!(approved.status, 200, "{}", approved.body);
            let left: Vec<String> = server.browser();
            assert_eq!(left.len(), 1 - answered, "{left:?}");
            assert!(left.iter().all(|cookie| !cookie.contains(id.as_str())));
        }
    }
}

#[tokio::test]
async fn turning_a_kind_of_app_off_stops_its_waiting_requests_and_codes() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50014/callback";
    let choose = |allowed: bool| {
        server.as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"claude-code","allowed":allowed}),
        )
    };
    // A request waiting when its kind is turned off cannot be approved; it can still be
    // approved once the kind is on again.
    let request = server.requested(CLAUDE_CODE, local).await;
    assert_eq!(choose(false).await.status, 200);
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Claude Code"),
        )
        .await;
    assert_eq!(
        (refused.status, s(&refused.body, "error")),
        (403, "app_not_allowed")
    );
    assert_eq!(choose(true).await.status, 200);
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    // A code not yet exchanged when its kind is turned off is refused, and makes nothing.
    assert_eq!(choose(false).await.status, 200);
    let exchanged = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(
        (exchanged.status, s(&exchanged.body, "error")),
        (400, "invalid_grant")
    );
    assert!(live_apps(&server, &owner).await.is_empty());
    assert_eq!(choose(true).await.status, 200);
    let exchanged = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(exchanged.status, 200, "{}", exchanged.body);
    // An app that registered itself is held to its own kind the same way.
    let cursor = "cursor://anysphere.cursor-retrieval/oauth/callback";
    let registered_app = registered(&server, "Cursor", cursor).await;
    let request = server.requested(&registered_app, cursor).await;
    let local_off = server
        .as_owner(
            &owner,
            "POST",
            "/api/platform/oauth/apps",
            json!({"id":"local","allowed":false}),
        )
        .await;
    assert_eq!(local_off.status, 200);
    let refused = server
        .as_owner(
            &owner,
            "POST",
            &format!("/api/platform/oauth/requests/{request}/approve"),
            everything("Cursor"),
        )
        .await;
    assert_eq!(s(&refused.body, "error"), "app_not_allowed");
}

/// What the Agents page lists of one key or app, by name.
async fn listed_key(server: &Server, owner: &Owner, name: &str) -> Value {
    let listed = server
        .as_owner(owner, "GET", "/api/platform/agents", json!({}))
        .await;
    listed.body["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["name"] == name && key["revokedAt"].is_null())
        .unwrap_or_else(|| panic!("no {name} in {}", listed.body))
        .clone()
}

/// The choices an approval's code carries until the app redeems it.
async fn stored_choices(server: &Server, code: &str) -> Value {
    let hash = crypto::sha(code);
    let row = server
        .state
        .read(move |db| {
            db.platform
                .one("SELECT choices FROM oauth_codes WHERE hash=?", [hash])
        })
        .await
        .unwrap()
        .unwrap();
    serde_json::from_str(s(&row, "choices")).unwrap()
}

#[tokio::test]
async fn an_approval_keeps_what_it_reads_and_one_from_before_reads_everything() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let local = "http://localhost:50020/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let code = server
        .approved(
            &owner,
            &request,
            json!({"name":"Laptop","allDsps":true,"dsps":[],
                "reads":{"areas":["timecards","routes"],"bypass":true}}),
        )
        .await;
    // The code carries the choices, reads among them, until the app redeems it; with every
    // tool and the addresses as chosen, as an older release redeems one in a rollback.
    assert_eq!(
        stored_choices(&server, &code).await,
        json!({"name":"Laptop","all_dsps":true,"dsps":[],
            "reads":{"areas":["routes","timecards"],"bypass":true},
            "tools":"full","locations":false})
    );
    let tokens = server.exchange(CLAUDE_CODE, local, &code).await;
    assert_eq!(tokens.status, 200, "{}", tokens.body);
    let app = listed_key(&server, &owner, "Laptop").await;
    assert_eq!(
        (&app["reads"], &app["dspReads"]),
        (
            &json!({"areas":["routes","timecards"],"bypass":true}),
            &json!([])
        )
    );

    // A code approved before this release, with tools and addresses, reads every kind of
    // data, addresses as they were chosen, and bypasses nothing.
    for (name, locations, areas) in [
        ("Desk", true, EVERY.to_vec()),
        (
            "Tablet",
            false,
            EVERY[..1].iter().chain(&EVERY[2..]).copied().collect(),
        ),
    ] {
        let request = server.requested(CHATGPT, CHATGPT_REDIRECT).await;
        let code = server.approved(&owner, &request, everything(name)).await;
        let stored = stored_choices(&server, &code).await;
        assert_eq!(
            (&stored["tools"], &stored["locations"]),
            (&json!("full"), &json!(true))
        );
        let old = json!({"name":name,"all_dsps":true,"dsps":[],"tools":"essential",
            "locations":locations})
        .to_string();
        let hash = crypto::sha(&code);
        server
            .state
            .run(move |db| {
                db.platform
                    .exec("UPDATE oauth_codes SET choices=? WHERE hash=?", [old, hash])?;
                Ok(())
            })
            .await
            .unwrap();
        let tokens = server.exchange(CHATGPT, CHATGPT_REDIRECT, &code).await;
        assert_eq!(tokens.status, 200, "{}", tokens.body);
        let app = listed_key(&server, &owner, name).await;
        assert_eq!(
            app["reads"],
            json!({"areas":areas,"bypass":false}),
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_connected_app_is_edited_like_a_key_but_only_ever_reads_and_never_expires() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let summit = server.dsp("Summit Delivery").await;
    let local = "http://localhost:50021/callback";
    let tokens = server
        .connect(
            &owner,
            CLAUDE_CODE,
            local,
            json!({"name":"Laptop","allDsps":false,"dsps":[north],
                "reads":{"areas":["routes","timecards"],"bypass":false}}),
        )
        .await;
    let access = s(&tokens, "access_token").to_owned();
    let id = s(&listed_key(&server, &owner, "Laptop").await, "id").to_owned();
    let path = format!("/api/platform/agents/keys/{id}");
    let edit = |change: Value| {
        let mut body = json!({"name":"Laptop","allDsps":false,"dsps":[north],"access":"read",
            "reads":{"areas":["routes","timecards"],"bypass":false},"dspReads":[],
            "expiresAt":null});
        for (field, value) in change.as_object().unwrap() {
            body[field] = value.clone();
        }
        body
    };
    // Never an operator, never an expiry, and never settings for a DSP it doesn't reach.
    for refused in [
        json!({"access":"operator"}),
        json!({"expiresAt":"2030-01-01T00:00:00Z"}),
        json!({"dspReads":[{"dsp":summit,"areas":[],"bypass":false}]}),
        json!({"dspReads":[{"dsp":north,"areas":[],"bypass":false},
            {"dsp":north,"areas":["dvic"],"bypass":false}]}),
    ] {
        let answer = server
            .as_owner(&owner, "POST", &path, edit(refused.clone()))
            .await;
        assert_eq!(
            (answer.status, s(&answer.body, "error")),
            (400, "invalid_input"),
            "{refused}"
        );
    }
    // Renamed, reaching another DSP, with Summit's own settings: the next call reads them.
    let edited = server
        .as_owner(
            &owner,
            "POST",
            &path,
            edit(json!({"name":"Laptop – Claude Code","dsps":[north, summit],
                "dspReads":[{"dsp":summit,"areas":["dvic"],"bypass":true}]})),
        )
        .await;
    assert_eq!(edited.status, 200, "{}", edited.body);
    assert_eq!(edited.body["kind"], "app");
    assert_eq!(
        edited.body["dspReads"],
        json!([{"dsp":summit,"areas":["dvic"],"bypass":true}])
    );
    let whoami = server.bearer("/api/v1/whoami", &access).await;
    let reads: Vec<(&str, &Value)> = whoami.body["dsps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dsp| (s(dsp, "name"), &dsp["reads"]))
        .collect();
    assert_eq!(
        reads,
        [
            (
                "Northline Logistics",
                &json!({"areas":["routes","timecards"],"bypass":false})
            ),
            ("Summit Delivery", &json!({"areas":["dvic"],"bypass":true})),
        ]
    );
    assert_eq!(
        logged(&server, "agent.app_updated").await,
        [(
            "agent.app_updated".to_owned(),
            "Laptop – Claude Code".to_owned()
        )]
    );
}

#[tokio::test]
async fn agents_ask_for_no_recent_verification_while_removing_a_dsp_still_does() {
    let server = Server::paired().await;
    let earlier = server.signed_in(db::now() - DAY).await;
    let north = server.dsp("Northline Logistics").await;
    let key = |name: &str| {
        json!({"name":name,"allDsps":true,"dsps":[],"access":"read",
            "reads":{"areas":["routes"],"bypass":false},"dspReads":[],"expiresAt":null})
    };
    // A day after signing in, the owner makes and edits a key, approves an app and edits it.
    let made = server
        .as_owner(&earlier, "POST", "/api/platform/agents/keys", key("Desk"))
        .await;
    assert_eq!(made.status, 200, "{}", made.body);
    let id = s(&made.body["key"], "id");
    let mut wider = key("Desk");
    wider["reads"]["bypass"] = json!(true);
    let edited = server
        .as_owner(
            &earlier,
            "POST",
            &format!("/api/platform/agents/keys/{id}"),
            wider,
        )
        .await;
    assert_eq!(edited.status, 200, "{}", edited.body);
    let local = "http://localhost:50022/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let code = server
        .approved(&earlier, &request, everything("Laptop"))
        .await;
    assert_eq!(server.exchange(CLAUDE_CODE, local, &code).await.status, 200);
    let app = s(&listed_key(&server, &earlier, "Laptop").await, "id").to_owned();
    let mut narrower = key("Laptop");
    narrower["reads"]["areas"] = json!(["dvic"]);
    let app_edited = server
        .as_owner(
            &earlier,
            "POST",
            &format!("/api/platform/agents/keys/{app}"),
            narrower,
        )
        .await;
    assert_eq!(app_edited.status, 200, "{}", app_edited.body);
    // Removing a DSP and the account's own security still ask.
    for (path, body) in [
        (format!("/api/platform/dsps/{north}/remove"), json!({})),
        (
            "/api/auth/security/authenticator/register/start".to_owned(),
            json!({}),
        ),
    ] {
        let stale = server.as_owner(&earlier, "POST", &path, body).await;
        assert_eq!(
            (stale.status, s(&stale.body, "error")),
            (403, "sign_in_again"),
            "{path}"
        );
    }
}
