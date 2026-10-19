//! A DSP's Settings over HTTP, as members reach it: nothing of one DSP is read, saved or
//! evicted from another, whatever a request names, and a member's own values are theirs.
//! A program of its own, as its registry holds a stand-in feature that declares sections,
//! and a program holds one registry.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    manifest::{Feature, feature, mandatory, settings::number, settings::section},
    server::operations,
    tenancy::roles,
    testing,
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

/// Every DSP has it, with one section of the DSP's and one each member has.
static PROBE: Feature = Feature {
    switch: mandatory("probe", "Probe"),
    settings: &[
        section(
            "probe.limits",
            "Limits",
            roles::ACCESS,
            &[number("days", "Keep", 30, 3650, Some(90))],
        )
        .on("data"),
        section(
            "probe.mine",
            "Mine",
            roles::ACCESS,
            &[number("days", "Keep", 30, 3650, Some(90))],
        )
        .member()
        .on("general"),
    ],
    ..feature("probe")
};

struct Server {
    _root: tempfile::TempDir,
    port: u16,
    state: Arc<State>,
    client: reqwest::Client,
}
/// Someone signed in, at the address their account signs in at, looking at a DSP's view.
struct Who {
    cookie: String,
    csrf: String,
    view: String,
    /// A member's DSP's own address; none for the platform owner, at the admin's.
    host: Option<String>,
}
struct Answer {
    status: u16,
    body: Value,
}
impl Server {
    async fn start() -> Self {
        testing::install(&[], &[&PROBE]);
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let dashboard = root.path().join("dashboard");
        std::fs::create_dir_all(&dashboard).unwrap();
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
        config.invite_origin = format!("http://invite.localhost:{port}");
        config.dsp_origin = format!("http://{{code}}.localhost:{port}");
        let store = Store::initialize(config.clone()).unwrap();
        operations::seed(&store).unwrap();
        // A second member of Northline's, beside the one the fixtures have.
        let north = s(
            &store
                .platform
                .one("SELECT id FROM dsps WHERE code='nll'", [])
                .unwrap()
                .unwrap(),
            "id",
        )
        .to_owned();
        let second = store
            .create_user(
                Some(&north),
                "second@dispatch.test",
                "Sam",
                "Reyes",
                "Dispatch-demo-2026!",
            )
            .unwrap();
        let people = store.dsp(&north).unwrap();
        let role = roles::default_role(&people, &north, "member").unwrap();
        people
            .exec(
                "INSERT INTO memberships(id,user_id,dsp_id,role,role_id) VALUES (?,?,?,'member',?)",
                [crypto::id("mem").unwrap(), second.id, north, role],
            )
            .unwrap();
        drop(people);
        drop(store);
        let state = State::new(config).unwrap();
        let app = dispatch_core::server::http::router(state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            _root: root,
            port,
            state,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }
    async fn dsp(&self, code: &'static str) -> String {
        let read = move |db: &Store| {
            let row = db
                .platform
                .one("SELECT id FROM dsps WHERE code=?", [code])?;
            Ok(s(&row.unwrap(), "id").to_owned())
        };
        self.state.read(read).await.unwrap()
    }
    /// `email`, signed in at their address with a session written directly, looking at `dsp`.
    async fn signed_in(&self, email: &'static str, dsp: &str) -> Who {
        let raw = crypto::token().unwrap();
        let token = raw.clone();
        let code = self
            .state
            .run(move |db| {
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
                        db::now()
                    ],
                )?;
                Ok(code)
            })
            .await
            .unwrap();
        let mut who = Who {
            cookie: format!("dispatch_session={raw}"),
            csrf: String::new(),
            view: String::new(),
            host: code.map(|code| format!("{code}.localhost:{}", self.port)),
        };
        let session = self.send("GET", "/api/session", &who, None).await;
        assert_eq!(session.status, 200, "{}", session.body);
        who.csrf = s(&session.body, "csrf").to_owned();
        let view = self.open(&who, dsp).await;
        assert_eq!(view.status, 200, "{}", view.body);
        who.view = s(&view.body, "token").to_owned();
        who
    }
    async fn open(&self, who: &Who, dsp: &str) -> Answer {
        let body = json!({ "dspId": dsp });
        self.send("POST", "/api/session/dsp", who, Some(body)).await
    }
    async fn send(&self, method: &str, path: &str, who: &Who, body: Option<Value>) -> Answer {
        let host = who
            .host
            .clone()
            .unwrap_or_else(|| format!("127.0.0.1:{}", self.port));
        let method = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let mut request = self
            .client
            .request(method, format!("http://127.0.0.1:{}{path}", self.port))
            .header("host", &host)
            .header("origin", format!("http://{host}"))
            .header("cookie", &who.cookie)
            .header("x-csrf-token", &who.csrf);
        if !who.view.is_empty() {
            request = request.header("x-dispatch-view", &who.view);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body.to_string());
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap();
        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }
    /// `id` as `who` reads it: its days and revision, or none when they don't see it.
    async fn days(&self, who: &Who, id: &str) -> Option<(Value, i64)> {
        let answer = self.send("GET", "/api/dsp/settings", who, None).await;
        assert_eq!(answer.status, 200, "{}", answer.body);
        let sections = answer.body["sections"].as_array().unwrap().clone();
        let section = sections.into_iter().find(|section| section["id"] == id)?;
        Some((
            section["values"]["days"].clone(),
            section["revision"].as_i64().unwrap(),
        ))
    }
    async fn save(&self, who: &Who, path: &str, body: Value) -> Answer {
        self.send("POST", path, who, Some(body)).await
    }
}
fn days(days: i64, revision: i64) -> Value {
    json!({"values": {"days": days}, "revision": revision})
}

#[tokio::test]
async fn a_member_reaches_only_the_dsp_their_view_opened_whatever_a_request_names() {
    let server = Server::start().await;
    let (north, summit) = (server.dsp("nll").await, server.dsp("sum").await);
    let owner = server.signed_in("owner@dispatch.test", &summit).await;
    let mut member = server.signed_in("member@dispatch.test", &north).await;
    let limits = "/api/dsp/settings/probe.limits";

    // Summit isn't theirs to open, and its view, made for another's session, isn't theirs.
    assert_eq!(server.open(&member, &summit).await.status, 403);
    let own = std::mem::replace(&mut member.view, owner.view.clone());
    let read = server.send("GET", "/api/dsp/settings", &member, None).await;
    assert_ne!(read.status, 200, "{}", read.body);
    assert_ne!(server.save(&member, limits, days(120, 0)).await.status, 200);
    member.view = own;
    // At Summit's own address their session is no one's.
    let at_summit = Who {
        host: Some(format!("sum.localhost:{}", server.port)),
        view: owner.view.clone(),
        cookie: member.cookie.clone(),
        csrf: member.csrf.clone(),
    };
    assert_eq!(
        server.save(&at_summit, limits, days(120, 0)).await.status,
        401
    );

    // Naming Summit in the body is refused; in the query, it's Northline's own that's saved.
    let named = json!({"values": {"days": 120}, "revision": 0, "dspId": summit});
    assert_eq!(server.save(&member, limits, named).await.status, 400);
    let query = format!("{limits}?dspId={summit}&dsp={summit}");
    assert_eq!(server.save(&member, &query, days(150, 0)).await.status, 200);

    assert_eq!(
        server.days(&member, "probe.limits").await,
        Some((json!(150), 1))
    );
    assert_eq!(
        server.days(&owner, "probe.limits").await,
        Some((json!(90), 0))
    );
}

#[tokio::test]
async fn a_save_in_one_dsp_leaves_anothers_values_and_cached_reads_alone() {
    let server = Server::start().await;
    let (north, summit) = (server.dsp("nll").await, server.dsp("sum").await);
    let at_north = server.signed_in("owner@dispatch.test", &north).await;
    let at_summit = server.signed_in("owner@dispatch.test", &summit).await;
    assert_eq!(
        server.days(&at_summit, "probe.limits").await,
        Some((json!(90), 0))
    );

    // Summit's record changes behind the cache's back, as nothing in the product writes it.
    let record = json!({"revision": 1, "values": {"days": 400}});
    let summit_id = summit.clone();
    let stored = record.clone();
    server
        .state
        .read(move |db| db.dsp(&summit_id)?.set("section:probe.limits", &stored))
        .await
        .unwrap();
    let saved = server.save(&at_north, "/api/dsp/settings/probe.limits", days(120, 0));
    assert_eq!(saved.await.status, 200);

    // Summit's cached read is still the one from before Northline's save, and its record is
    // as it was.
    assert_eq!(
        server.days(&at_summit, "probe.limits").await,
        Some((json!(90), 0))
    );
    let summit_id = summit.clone();
    let kept = server
        .state
        .read(move |db| {
            db.dsp(&summit_id)?
                .setting("section:probe.limits", Value::Null)
        })
        .await
        .unwrap();
    assert_eq!(kept, record);
    assert_eq!(
        server.days(&at_north, "probe.limits").await,
        Some((json!(120), 1))
    );
    // Summit's own save evicts its read.
    let saved = server.save(&at_summit, "/api/dsp/settings/probe.limits", days(500, 1));
    assert_eq!(saved.await.status, 200);
    assert_eq!(
        server.days(&at_summit, "probe.limits").await,
        Some((json!(500), 2))
    );
}

#[tokio::test]
async fn one_members_own_values_are_not_anothers() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let first = server.signed_in("member@dispatch.test", &north).await;
    let second = server.signed_in("second@dispatch.test", &north).await;
    let mine = "/api/dsp/settings/probe.mine";
    assert_eq!(server.save(&first, mine, days(45, 0)).await.status, 200);
    assert_eq!(server.save(&second, mine, days(60, 0)).await.status, 200);
    // The first's answer is cached as the second, of the same role, reads theirs.
    assert_eq!(
        server.days(&first, "probe.mine").await,
        Some((json!(45), 1))
    );
    assert_eq!(
        server.days(&second, "probe.mine").await,
        Some((json!(60), 1))
    );
    // The DSP's own is still the DSP's.
    assert_eq!(
        server.days(&second, "probe.limits").await,
        Some((json!(90), 0))
    );
}
