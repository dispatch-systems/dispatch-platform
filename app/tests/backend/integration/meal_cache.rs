//! The meal-break page's cache and live results over HTTP, against the real router served
//! in-process over a loopback socket: Timecard's page, Paycom's live collection and core's
//! cache and leases together.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    server::operations,
};
use dispatch_paycom as paycom;
use dispatch_timecard::TimecardStore;
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

const SCRIPT: &str = "/assets/app-abcd1234.js";

struct Server {
    _root: tempfile::TempDir,
    origin: String,
    state: Arc<State>,
    client: reqwest::Client,
}
struct Who {
    cookie: String,
    csrf: String,
    view: Option<String>,
}
struct Answer {
    status: u16,
    body: Value,
}
impl Answer {
    fn error(&self) -> &str {
        self.body["error"].as_str().unwrap_or("")
    }
}
struct Call<'a> {
    method: &'a str,
    path: &'a str,
    who: Option<&'a Who>,
    body: Option<String>,
    headers: Vec<(&'a str, String)>,
    csrf: bool,
    origin: bool,
}
impl<'a> Call<'a> {
    fn new(method: &'a str, path: &'a str) -> Self {
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
    fn get(path: &'a str) -> Self {
        Self::new("GET", path)
    }
    fn post(path: &'a str, body: Value) -> Self {
        Self::new("POST", path).raw(body.to_string())
    }
    fn raw(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }
    fn who(mut self, who: &'a Who) -> Self {
        self.who = Some(who);
        self
    }
}
impl Server {
    async fn start() -> Self {
        dispatch_backend::install();
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let dashboard = root.path().join("dashboard");
        std::fs::create_dir_all(dashboard.join("assets")).unwrap();
        std::fs::write(dashboard.join("index.html"), "<html><head></head></html>").unwrap();
        std::fs::write(dashboard.join(&SCRIPT[1..]), "console.log(1)").unwrap();
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
            _root: root,
            origin: format!("http://127.0.0.1:{port}"),
            state,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }
    // A session row written directly, so tests do not pay for password hashing.
    async fn session(&self, email: &'static str) -> Who {
        let raw = crypto::token().unwrap();
        let token = raw.clone();
        self.state
            .run(move |db| {
                let user = db
                    .platform
                    .one("SELECT id,version FROM users WHERE email=?", [email])?
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
        let mut who = Who {
            cookie: format!("dispatch_session={raw}"),
            csrf: String::new(),
            view: None,
        };
        let session = self.send(Call::get("/api/session").who(&who)).await;
        assert_eq!(session.status, 200, "{}", session.body);
        who.csrf = s(&session.body, "csrf").to_owned();
        who
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
    async fn member(&self, email: &'static str) -> Who {
        let mut who = self.session(email).await;
        let dsp = self.dsp("Northline Logistics").await;
        let view = self
            .send(Call::post("/api/session/dsp", json!({"dspId":dsp})).who(&who))
            .await;
        assert_eq!(view.status, 200, "{}", view.body);
        who.view = Some(s(&view.body, "token").to_owned());
        who
    }
    async fn send(&self, call: Call<'_>) -> Answer {
        let method = reqwest::Method::from_bytes(call.method.as_bytes()).unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        let mut set = |name: &str, value: &str| {
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap();
            headers.insert(name, value.parse().unwrap());
        };
        if call.origin {
            set("origin", &self.origin);
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
            .request(method, format!("{}{}", self.origin, call.path))
            .headers(headers);
        if let Some(body) = call.body {
            request = request.body(body);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap();
        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }
    async fn expect(&self, call: Call<'_>, status: u16, error: &str) {
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
}

#[tokio::test]
async fn meal_cache_rechecks_live_visibility_after_lease_bookkeeping_and_authorizes_hits() {
    use dispatch_paycom::PaycomStore;
    let server = Server::start().await;
    let member = server.member("member@dispatch.test").await;
    let dsp = server.dsp("Northline Logistics").await;
    let tenant = dsp.clone();
    let date: String = server
        .state
        .read(move |db| {
            let publication = db
                .collector(&tenant, paycom::PROVIDER)?
                .one("SELECT period_to FROM publications WHERE active=1", [])?
                .unwrap();
            Ok(s(&publication, "period_to").into())
        })
        .await
        .unwrap();
    let path = format!("/api/dsp/paycom/meal-breaks?date={date}");
    let before = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(before.status, 200, "{}", before.body);
    // Even a populated DSP cache cannot lend its reader's authorization to another call.
    server
        .expect(Call::get(&path), 401, "sign_in_required")
        .await;
    let tenant = dsp.clone();
    let day = date.clone();
    let job = server
        .state
        .run_scoped(dsp.clone(), dispatch_timecard::DOMAIN, move |db| {
            let queued = db.enqueue_timecards(&tenant, None, "meal-cache-live")?;
            let job = db
                .claim_job("meal-cache-owner", |id, provider| {
                    id == tenant && provider == paycom::PROVIDER
                })?
                .unwrap();
            assert_eq!(job.id, s(&queued, "id"));
            let employee = db
                .collector(&tenant, paycom::PROVIDER)?
                .one(
                    "SELECT e.* FROM employees e JOIN publications p ON p.id=e.publication_id \
             WHERE p.active=1 AND e.code='E001'",
                    [],
                )?
                .unwrap();
            db.start_live(
                &job.id,
                "meal-cache-owner",
                &json!({"from":day,"to":day,"roster":[employee.clone()]}),
            )?;
            db.stage_paycom(
                &job.id,
                "meal-cache-owner",
                &employee,
                &[json!({
                    "employeeCode":"E001","date":day,"hours":9,"status":"Complete",
                    "punches":[{"in":"09:11","out":"18:11","hours":9}]
                })],
            )?;
            Ok(job.id)
        })
        .await
        .unwrap();
    let live = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(live.status, 200, "{}", live.body);
    let live_row = live.body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "paycom:E001")
        .unwrap();
    assert_eq!(live_row["paycom"]["punches"][0]["in"], "09:11");
    server
        .state
        .run_bookkeeping(move |db| {
            db.jobs
                .exec("UPDATE jobs SET lease_until=0 WHERE id=?", [job])?;
            Ok(())
        })
        .await
        .unwrap();
    let expired = server.send(Call::get(&path).who(&member)).await;
    assert_eq!(expired.status, 200, "{}", expired.body);
    assert_eq!(
        expired.body, before.body,
        "expired live data must immediately return to the publication"
    );
}
