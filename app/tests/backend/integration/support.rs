//! What the app's integration tests share: the real router served in-process over a loopback
//! socket, on the development fixtures; sessions written directly, so tests skip password
//! hashing; and requests sent as a browser or an agent sends them. A test file with helpers of
//! its own wraps `Server` in a `Server` of its own that derefs to this one.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    server::operations,
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

/// The dashboard's one script, by the hashed name a build gives it.
pub const SCRIPT: &str = "/assets/app-abcd1234.js";
/// The platform owner's account in the development fixtures.
pub const OWNER: &str = "owner@dispatch.test";
/// The DSP `Server::member` views.
const NORTHLINE: &str = "Northline Logistics";

/// The router alone, without the scheduler, so nothing is collected or written down unless a
/// test does it.
pub struct Server {
    _root: tempfile::TempDir,
    port: u16,
    /// The admin's address.
    pub origin: String,
    pub state: Arc<State>,
    /// Never follows a redirect, so a test reads where one goes.
    pub client: reqwest::Client,
}
/// Someone signed in: their session's cookie and CSRF token, and the DSP they view, if any.
pub struct Who {
    pub cookie: String,
    pub csrf: String,
    pub view: Option<String>,
    /// A member's DSP's own address, where their session was made: none for the platform owner.
    pub host: Option<String>,
}
/// What a request answered: its status, its body read as JSON (null when it isn't), its headers
/// and how many bytes its body was.
pub struct Answer {
    pub status: u16,
    pub body: Value,
    pub headers: reqwest::header::HeaderMap,
    pub bytes: usize,
}
impl Answer {
    /// The error its body names, or nothing.
    pub fn error(&self) -> &str {
        self.body["error"].as_str().unwrap_or("")
    }
    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }
}
/// A request as the dashboard makes it: from its caller's address, with their session, CSRF
/// token and view, unless a test leaves one out.
pub struct Call<'a> {
    method: &'a str,
    path: &'a str,
    who: Option<&'a Who>,
    body: Option<String>,
    headers: Vec<(&'a str, String)>,
    csrf: bool,
    origin: bool,
}
impl<'a> Call<'a> {
    pub fn new(method: &'a str, path: &'a str) -> Self {
        Self {
            method,
            path,
            who: None,
            body: None,
            headers: vec![],
            csrf: true,
            origin: true,
        }
    }
    pub fn get(path: &'a str) -> Self {
        Self::new("GET", path)
    }
    pub fn post(path: &'a str, body: Value) -> Self {
        Self::new("POST", path).raw(body.to_string())
    }
    pub fn raw(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }
    pub fn who(mut self, who: &'a Who) -> Self {
        self.who = Some(who);
        self
    }
    pub fn header(mut self, name: &'a str, value: &str) -> Self {
        self.headers.push((name, value.to_owned()));
        self
    }
    /// Only tests that need Timecard and Team send a write without its CSRF token.
    #[cfg(all(feature = "timecard", feature = "team"))]
    pub fn without_csrf(mut self) -> Self {
        self.csrf = false;
        self
    }
    pub fn without_origin(mut self) -> Self {
        self.origin = false;
        self
    }
}
impl Server {
    /// The router on the development fixtures.
    pub async fn start() -> Self {
        Self::start_with(|_| ()).await.0
    }
    /// The router on the development fixtures once `prepare` has changed them, before the
    /// server opens them, and what `prepare` answered.
    pub async fn start_with<T>(prepare: impl FnOnce(&Store) -> T) -> (Self, T) {
        dispatch_backend::install();
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let dashboard = root.path().join("dashboard");
        std::fs::create_dir_all(dashboard.join("assets")).unwrap();
        std::fs::write(dashboard.join("index.html"), "<html><head></head></html>").unwrap();
        std::fs::write(dashboard.join(&SCRIPT[1..]), "console.log(1)").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let origin = format!("http://127.0.0.1:{port}");
        let mut config = Config::load().unwrap();
        config.root = root.path().join("state");
        db::private_dir(&config.root).unwrap();
        config.development = true;
        config.fixture = true;
        config.environment = "preview".into();
        config.standalone = true;
        config.dashboard = dashboard;
        config.port = port;
        config.origin = origin.clone();
        config.invite_origin = format!("http://invite.localhost:{port}");
        config.dsp_origin = format!("http://{{code}}.localhost:{port}");
        let store = Store::initialize(config.clone()).unwrap();
        operations::seed(&store).unwrap();
        let prepared = prepare(&store);
        drop(store);
        let state = State::new(config).unwrap();
        let app = dispatch_core::server::http::router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let server = Self {
            _root: root,
            port,
            origin,
            state,
            client,
        };
        (server, prepared)
    }
    /// `path` at the admin's address.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }
    /// A DSP's own address, by its code, as `dsp_origin` names it.
    pub fn host(&self, code: &str) -> String {
        format!("{code}.localhost:{}", self.port)
    }
    /// A DSP's id, by its name.
    pub async fn dsp(&self, name: &'static str) -> String {
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
    /// The platform owner's session, at the admin's address.
    pub async fn owner(&self) -> Who {
        self.session(OWNER).await
    }
    /// `email`'s session, signed in now.
    pub async fn session(&self, email: &'static str) -> Who {
        self.signed_in(email, db::now()).await
    }
    /// `email`'s session, signed in at `created_at`: a session row written directly, used at the
    /// address its account signs in at, the admin's or its DSP's own.
    pub async fn signed_in(&self, email: &'static str, created_at: i64) -> Who {
        let raw = crypto::token().unwrap();
        let token = raw.clone();
        let code = self
            .state
            .run(move |db| {
                // A platform owner's account is the platform's; anyone else's, its DSP's own.
                let find = "SELECT id,version FROM users WHERE email=?";
                let mut found = db
                    .platform
                    .one(&format!("{find} AND platform_owner=1"), [email])?
                    .map(|user| (None, None, user));
                for dsp in db.platform.all("SELECT id,code FROM dsps", [])? {
                    if found.is_none()
                        && let Some(user) = db.dsp(s(&dsp, "id"))?.one(find, [email])?
                    {
                        let code = dsp["code"].as_str().map(str::to_owned);
                        found = Some((Some(s(&dsp, "id").to_owned()), code, user));
                    }
                }
                let (scope, code, user) = found.unwrap();
                db.directory(scope.as_deref())?.exec(
                    "INSERT INTO sessions VALUES (?,?,?,?,?)",
                    rusqlite::params![
                        crypto::sha(&token),
                        s(&user, "id"),
                        db::n(&user, "version"),
                        db::now() + 600000,
                        created_at
                    ],
                )?;
                Ok(code)
            })
            .await
            .unwrap();
        let mut who = Who {
            cookie: format!("dispatch_session={raw}"),
            csrf: String::new(),
            view: None,
            host: code.map(|code| self.host(&code)),
        };
        let session = self.send(Call::get("/api/session").who(&who)).await;
        assert_eq!(session.status, 200, "{}", session.body);
        who.csrf = s(&session.body, "csrf").to_owned();
        who
    }
    /// `email`'s session, viewing Northline.
    pub async fn member(&self, email: &'static str) -> Who {
        let mut who = self.session(email).await;
        let dsp = self.dsp(NORTHLINE).await;
        let view = self
            .send(Call::post("/api/session/dsp", json!({"dspId":dsp})).who(&who))
            .await;
        assert_eq!(view.status, 200, "{}", view.body);
        who.view = Some(s(&view.body, "token").to_owned());
        who
    }
    /// What `call` answers, sent as the dashboard sends it.
    pub async fn send(&self, call: Call<'_>) -> Answer {
        let method = reqwest::Method::from_bytes(call.method.as_bytes()).unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        let mut set = |name: &str, value: &str| {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap();
            headers.insert(name, value.parse().unwrap());
        };
        let host = call.who.and_then(|who| who.host.as_deref());
        if call.origin {
            set(
                "origin",
                &host.map_or_else(|| self.origin.clone(), |host| format!("http://{host}")),
            );
        }
        if let Some(host) = host {
            set("host", host);
        }
        if let Some(who) = call.who {
            set("cookie", &who.cookie);
            if call.csrf {
                set("x-csrf-token", &who.csrf);
            }
            if let Some(view) = &who.view {
                set("x-dispatch-view", view);
            }
        }
        if call.body.is_some() {
            set("content-type", "application/json");
        }
        for (name, value) in &call.headers {
            set(name, value);
        }
        let mut request = self
            .client
            .request(method, self.url(call.path))
            .headers(headers);
        if let Some(body) = call.body {
            request = request.body(body);
        }
        self.answer(request).await
    }
    /// What `request`, made on `client`, answers.
    pub async fn answer(&self, request: reqwest::RequestBuilder) -> Answer {
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = response.bytes().await.unwrap();
        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            headers,
            bytes: bytes.len(),
        }
    }
    /// Sends `call`, which must answer `status` with `error`, or with no error.
    pub async fn expect(&self, call: Call<'_>, status: u16, error: &str) {
        let label = format!("{} {}", call.method, call.path);
        let answer = self.send(call).await;
        assert_eq!(
            (answer.status, answer.error()),
            (status, error),
            "{label}: {}",
            answer.body
        );
        if !error.is_empty() && answer.body != Value::Null {
            assert_eq!(
                answer.body,
                json!({"error":error,"message":error.replace('_', " ")}),
                "{label}"
            );
        }
    }
    /// An agent's call to `path` with `token`, a key's or a connected app's.
    pub async fn bearer(&self, path: &str, token: &str) -> Answer {
        self.answer(
            self.client
                .get(self.url(path))
                .header("authorization", format!("Bearer {token}")),
        )
        .await
    }
    /// An agent's MCP request with `token`.
    pub async fn mcp(&self, token: &str, method: &str, params: Value) -> Answer {
        self.answer(
            self.client
                .post(self.url("/api/v1/mcp"))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-06-18")
                .body(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string()),
        )
        .await
    }
}
