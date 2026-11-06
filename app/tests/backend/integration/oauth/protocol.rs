use super::harness::{
    CHATGPT, CHATGPT_REDIRECT, CLAUDE_CODE, Server, VERIFIER, asked, everything, live_apps, param,
    registered,
};
use crate::support::Answer;
use dispatch_core::{
    db::{self, s},
    foundation::crypto,
};
use dispatch_mcp::{
    OAuthStore,
    oauth::network::{Network, Pending},
    piece::Kept,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Where a redirect goes: the Location's query parameters, after its `?`.
fn redirect_params(answer: &Answer) -> Vec<(String, String)> {
    let location = answer.header("location");
    let query = location.split_once('?').map_or("", |(_, query)| query);
    url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
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
            .answer(
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
        let params = redirect_params(&answer);
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
    let params = redirect_params(&server.authorize(&pairs).await);
    assert_eq!(param(&params, "error"), Some("invalid_request"));
    assert_eq!(param(&params, "state"), None);
    // MCP clients must name the protected resource. Scope may be omitted and defaults to the
    // one Dispatch grants; an unknown scope is never silently ignored.
    let fields = with(&[("resource", None), ("scope", None)]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let missing = redirect_params(&server.authorize(&pairs).await);
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
    let unknown = redirect_params(&server.authorize(&pairs).await);
    assert_eq!(param(&unknown, "error"), Some("invalid_scope"));
    let fields = with(&[("scope", Some("offline_access"))]);
    let pairs: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let incomplete = redirect_params(&server.authorize(&pairs).await);
    assert_eq!(param(&incomplete, "error"), Some("invalid_scope"));
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
        .answer(
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
        .answer(
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
async fn apps_register_themselves_only_to_come_back_to_this_computer() {
    let server = Server::paired().await;
    let owner = server.owner().await;
    let register = |body: Value| {
        server.answer(
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
async fn token_ingress_rejects_cross_site_browsers_and_stops_before_database_saturation() {
    let server = Server::start().await;
    let owner = server.owner().await;
    let resource = server.resource();
    let cross_site = server
        .answer(
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
    // Once the pre-database budget is spent, a read that does reach the database still answers.
    let agents = server
        .as_owner(&owner, "GET", "/api/platform/agents", json!({}))
        .await;
    assert_eq!(agents.status, 200, "{}", agents.body);
}

#[tokio::test]
async fn registration_rejects_cross_site_browsers_before_spending_its_budget() {
    let server = Server::paired().await;
    for _ in 0..20 {
        let refused = server
            .answer(
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
            .answer(
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
        .answer(
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
    Kept::of(&server.state)
        .documents
        .use_network(internet.clone());
    let page = |error: &str| format!("{}/#authorize?error={error}", server.origin);
    let register = |redirect: &str| {
        server.answer(
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
    let site = dispatch_mcp::oauth::network::FIXTURE_APP;
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
