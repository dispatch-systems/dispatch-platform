use crate::support::{self, Answer, Who};
use dispatch_core::{db::s, foundation::crypto, testing as common};
use dispatch_mcp::GuardStore;
use serde_json::{Value, json};

pub(super) const CLAUDE_CODE: &str = "https://claude.ai/oauth/claude-code-client-metadata";
pub(super) const CHATGPT: &str = "https://chatgpt.com/oauth/client.json";
pub(super) const CHATGPT_REDIRECT: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
pub(super) const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

/// The served router, with the owner's browser and what apps ask of it.
pub(super) struct Server {
    served: support::Server,
    /// The cookies the owner's browser keeps for the requests it brought from apps, one per
    /// request as `name=nonce`: set by the authorization endpoint, cleared by an answer, sent
    /// with the owner's calls.
    browser: std::sync::Mutex<Vec<String>>,
}

impl std::ops::Deref for Server {
    type Target = support::Server;
    fn deref(&self) -> &support::Server {
        &self.served
    }
}

pub(super) fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

impl Server {
    pub(super) async fn start() -> Self {
        Self {
            served: support::Server::start().await,
            browser: std::sync::Mutex::default(),
        }
    }
    /// A server whose platform owner has opened the pairing window, as apps need to ask.
    pub(super) async fn paired() -> Self {
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
    pub(super) async fn get(&self, path: &str) -> Answer {
        self.answer(self.client.get(self.url(path))).await
    }
    pub(super) async fn text(&self, path: &str) -> (u16, String) {
        let response = self.client.get(self.url(path)).send().await.unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }
    pub(super) async fn as_owner(
        &self,
        owner: &Who,
        method: &str,
        path: &str,
        body: Value,
    ) -> Answer {
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
        let answer = self.answer(request.header("cookie", cookie)).await;
        self.keep_browser(&answer);
        answer
    }
    /// The owner's browser's cookies for its requests, as it would send them.
    pub(super) fn browser(&self) -> Vec<String> {
        self.browser.lock().unwrap().clone()
    }
    pub(super) fn set_browser(&self, cookies: &[String]) {
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
    pub(super) async fn form(&self, path: &str, fields: &[(&str, &str)]) -> Answer {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        self.answer(
            self.client
                .post(self.url(path))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body),
        )
        .await
    }
    pub(super) async fn authorize(&self, fields: &[(&str, &str)]) -> Answer {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        let answer = self.get(&format!("/oauth/authorize?{query}")).await;
        // The owner's browser follows the app's link.
        self.keep_browser(&answer);
        answer
    }
    pub(super) fn resource(&self) -> String {
        format!("{}/api/v1/mcp", self.origin)
    }
    /// An authorization request as a client sends one, with its PKCE challenge and state.
    pub(super) fn request<'a>(
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
    pub(super) async fn requested(&self, client: &str, redirect: &str) -> String {
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
    pub(super) async fn approved(&self, owner: &Who, request: &str, choices: Value) -> String {
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
    pub(super) async fn exchange(&self, client: &str, redirect: &str, code: &str) -> Answer {
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
    pub(super) async fn refresh(&self, client: &str, token: &str) -> Answer {
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
    pub(super) async fn connect(
        &self,
        owner: &Who,
        client: &str,
        redirect: &str,
        choices: Value,
    ) -> Value {
        let request = self.requested(client, redirect).await;
        let code = self.approved(owner, &request, choices).await;
        let tokens = self.exchange(client, redirect, &code).await;
        assert_eq!(tokens.status, 200, "{}", tokens.body);
        tokens.body
    }
}

pub(super) fn everything(name: &str) -> Value {
    json!({"name":name,"allDsps":true,"dsps":[],"allTools":true,"tools":{}})
}

/// Registers an app as it would itself, answering its new client id.
pub(super) async fn registered(server: &Server, name: &str, redirect: &str) -> String {
    let answer = server
        .answer(
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
pub(super) async fn live_apps(server: &Server, owner: &Who) -> Vec<(String, String)> {
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

/// The location an authorization request is sent to, for `client` at `redirect`.
pub(super) async fn asked(server: &Server, client: &str, redirect: &str) -> String {
    let challenge = crypto::s256(VERIFIER);
    let fields = server.request(client, redirect, &challenge);
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let answer = server.authorize(&pairs).await;
    assert_eq!(answer.status, 302, "{}", answer.body);
    answer.header("location").to_owned()
}

/// The owner's audited actions with this prefix, oldest first.
pub(super) async fn logged(server: &Server, prefix: &'static str) -> Vec<(String, String)> {
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
