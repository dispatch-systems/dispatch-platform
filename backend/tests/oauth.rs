//! Sign in with Dispatch, over HTTP against the real router: discovery, the authorization
//! request and its refusals, the owner's approval, the token endpoint, revocation, and the
//! connected app signing in to the agent API and MCP like a key.
mod common;
use dispatch_backend::{
    State,
    config::Config,
    crypto,
    db::{self, Store, s},
    operations,
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

const CLAUDE_CODE: &str = "https://claude.ai/oauth/claude-code-client-metadata";
const CHATGPT: &str = "https://chatgpt.com/oauth/client.json";
const CHATGPT_REDIRECT: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

struct Server {
    _root: tempfile::TempDir,
    origin: String,
    state: Arc<State>,
    client: reqwest::Client,
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
        let app = dispatch_backend::http::router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
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
    /// A session row written directly, so tests do not pay for password hashing.
    async fn owner(&self) -> Owner {
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
                        db::now()
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
        self.send(request.header("cookie", &owner.cookie)).await
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
        self.get(&format!("/oauth/authorize?{query}")).await
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
fn everything(name: &str) -> Value {
    json!({"name":name,"allDsps":true,"dsps":[],"tools":"full","locations":true})
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
    let server = Server::start().await;
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
    let server = Server::start().await;
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
    // Without a resource or scope, the request stands: both are what Dispatch grants.
    let fields = with(&[("resource", None), ("scope", None)]);
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
}

#[tokio::test]
async fn the_owner_approves_once_and_the_code_is_redeemed_once() {
    let server = Server::start().await;
    let owner = server.owner().await;
    // Claude Code's document lists portless loopback redirects; any port is the same one.
    let local = "http://localhost:61234/callback";
    let request = server.requested(CLAUDE_CODE, local).await;
    let path = format!("/api/platform/oauth/requests/{request}");
    let shown = server.as_owner(&owner, "GET", &path, json!({})).await;
    assert_eq!(shown.status, 200, "{}", shown.body);
    assert_eq!(
        shown.body["app"],
        json!({"name":"Claude Code","clientId":CLAUDE_CODE,"verified":true,
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
    let code = server
        .approved(&owner, &request, everything("Claude Code"))
        .await;
    // Answered, the request is gone.
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
        json!({"name":"Claude Code","verified":true,"status":"connected"})
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
    let server = Server::start().await;
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
    assert_eq!(
        actions,
        [
            "agent.app_connected",
            "agent.app_revoked",
            "agent.app_connected"
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
    let server = Server::start().await;
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
            json!({"name":"Desk key","allDsps":true,"dsps":[],"access":"read","tools":"full",
                "locations":false,"expiresAt":null}),
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
    let server = Server::start().await;
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
async fn an_unverified_app_never_replaces_a_known_one() {
    let server = Server::start().await;
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
    // Nor does a known app replace an unverified one that took a name.
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
    let server = Server::start().await;
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
    // One given a token no longer counts against the cap.
    server
        .state
        .run(|db| {
            db.platform.exec(
                "UPDATE oauth_clients SET last_used_at=? WHERE id=?",
                [db::iso(), format!("dcr_{:032}", 0)],
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
async fn the_approval_page_says_what_approving_would_replace() {
    let server = Server::start().await;
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
    let server = Server::start().await;
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(CLAUDE_CODE, "http://localhost:1/callback", &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    for _ in 0..60 {
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
async fn a_replay_with_the_wrong_verifier_ends_nothing() {
    let server = Server::start().await;
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
            ],
        )
        .await;
    assert_eq!(s(&replay.body, "error"), "invalid_grant");
    let access = s(&tokens.body, "access_token");
    assert_eq!(server.bearer("/api/v1/whoami", access).await.status, 200);
}

#[tokio::test]
async fn the_owner_can_deny_and_the_app_is_told() {
    let server = Server::start().await;
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
async fn refresh_tokens_rotate_with_a_minute_of_grace() {
    let server = Server::start().await;
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
    // Within the minute, the same token brings another pair, for parallel refreshes.
    let parallel = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(parallel.status, 200, "{}", parallel.body);
    for pair in [&second.body, &parallel.body] {
        let token = s(pair, "access_token");
        assert_eq!(server.bearer("/api/v1/whoami", token).await.status, 200);
    }
    // After it, presenting it again ends the app and every token it has.
    let hash = crypto::sha(refresh);
    server
        .state
        .run(move |db| {
            db.platform.exec(
                "UPDATE oauth_tokens SET used_at=? WHERE hash=?",
                [db::at(db::now() - 61_000), hash],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let late = server.refresh(CLAUDE_CODE, refresh).await;
    assert_eq!(s(&late.body, "error"), "invalid_grant");
    for pair in [&second.body, &parallel.body] {
        assert_eq!(
            server
                .bearer("/api/v1/whoami", s(pair, "access_token"))
                .await
                .status,
            401
        );
        let gone = server.refresh(CLAUDE_CODE, s(pair, "refresh_token")).await;
        assert_eq!(s(&gone.body, "error"), "invalid_grant");
    }
}

#[tokio::test]
async fn expired_or_revoked_access_ends_with_invalid_token() {
    let server = Server::start().await;
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
                "tools":"full","locations":true,"expiresAt":null}),
        )
        .await;
    assert_eq!(s(&edited.body, "error"), "connected_app_not_editable");
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
    let server = Server::start().await;
    let owner = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let tokens = server
        .connect(
            &owner,
            CHATGPT,
            CHATGPT_REDIRECT,
            json!({"name":"ChatGPT","allDsps":false,"dsps":[north],"tools":"essential",
                "locations":false}),
        )
        .await;
    let access = s(&tokens, "access_token");
    let whoami = server.bearer("/api/v1/whoami", access).await;
    assert_eq!(whoami.status, 200, "{}", whoami.body);
    assert_eq!(
        whoami.body["key"],
        json!({"name":"ChatGPT","access":"read","tools":"essential","locations":false,
            "expiresAt":null})
    );
    let dsps: Vec<&str> = whoami.body["dsps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|dsp| s(dsp, "id"))
        .collect();
    assert_eq!(dsps, [north.as_str()]);
    // MCP offers the essential tools only, and answers whoami the same way.
    let listed = server.mcp(access, "tools/list").await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let tools: Vec<&str> = listed.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| s(tool, "name"))
        .collect();
    let essential: Vec<&str> = dispatch_backend::agents::data::catalog::ENDPOINTS
        .iter()
        .filter(|endpoint| endpoint.essential)
        .map(|endpoint| endpoint.tool)
        .collect();
    assert_eq!(tools, essential);
    // Its calls count like a key's, under the connected app.
    let used = server.state.agents.last();
    let listed = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    let app = &listed.body["keys"][0];
    assert!(used.contains_key(s(app, "id")));
    assert!(app["lastUsedAt"].is_string());
    assert_eq!(app["dsps"], json!([north]));
}

#[tokio::test]
async fn apps_register_themselves_only_to_come_back_to_this_computer() {
    let server = Server::start().await;
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
        json!({"name":"Cursor","clientId":cursor_id,"verified":false,
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
    // A loopback registration takes any port; it is shown as unverified, by its own name.
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
    assert_eq!(shown.body["app"]["verified"], false);
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
    let server = Server::start().await;
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
            &[("grant_type", "password"), ("client_id", CLAUDE_CODE)],
        )
        .await;
    assert_eq!(s(&grant.body, "error"), "unsupported_grant_type");
}

#[test]
fn what_can_no_longer_be_used_is_pruned() {
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
                "INSERT INTO oauth_requests VALUES (?,'c','C',1,'r',NULL,'x','res','dispatch',?,?)",
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
    // A day's grace for a registration never used.
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
    assert_eq!(
        left("oauth_clients", "id"),
        ["dcr_new", "dcr_today", "dcr_used"]
    );
}
