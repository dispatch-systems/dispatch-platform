//! A DSP's Settings over HTTP, as members reach it: nothing of one DSP is read, saved or
//! evicted from another, whatever a request names, and a member's own values are theirs.
//! A program of its own, as its registry holds a stand-in feature that declares sections,
//! and a program holds one registry.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{
        config::{Config, Environment},
        crypto,
    },
    manifest::{Feature, feature, mandatory, settings::number, settings::section},
    server::{operations, startup},
    tenancy::roles,
    testing,
};
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::Duration,
};

type Maintained = (String, bool, dispatch_core::Result<usize>);
static MAINTAINED: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Maintained>>> = Mutex::new(None);

/// Every DSP has it, with one section of the DSP's and one each member has.
static PROBE: Feature = Feature {
    switch: mandatory("probe", "Probe"),
    page_sources: &[dispatch_core::manifest::page_source(
        "probe.images",
        dispatch_core::manifest::Loads::Images,
        "https://shared-media.example.test",
    )],
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
    routes: || {
        vec![dispatch_core::server::http::route::protocol(
            axum::http::Method::POST,
            "/shared-protocol",
            raw_callback,
        )]
    },
    ..feature("probe")
};

// A test-only custom feature exercises the same generated registration and HTTP dispatch as
// an app feature, without adding a product feature to any build.
static CUSTOM: Feature = Feature {
    switch: dispatch_core::manifest::custom("nll_probe", "Owner probe", &[]),
    page_sources: &[
        dispatch_core::manifest::page_source("nll_probe.images", dispatch_core::manifest::Loads::Images, "https://owner-media.example.test"),
        dispatch_core::manifest::page_source("nll_probe.calls", dispatch_core::manifest::Loads::Calls, "https://owner-calls.example.test"),
    ],
    permissions: &[dispatch_core::manifest::perm("nll_probe.view", "View probe", 1).group("Probe").implies(&["sum_probe.view"]).under("sum_probe.view")],
    settings: &[section("nll_probe.limits", "Limits", "nll_probe.view", &[number("days", "Keep", 30, 3650, Some(90))])
        .aliases(&["nll_next_probe.limits"]).on("data")],
    settings_keys: &[dispatch_core::manifest::StoredKey::new("nll_probe.kept", &["nll_next_probe.kept"])],
    own_migrations: &[dispatch_core::db::OwnMigrations {
        kind: dispatch_core::db::Kind::DSP,
        list: &[dispatch_core::db::Migration {
            id: 1,
            name: "probe",
            apply: dispatch_core::db::migrations::Apply::Sql("CREATE TABLE nll_probe (value INTEGER NOT NULL)"),
        }],
    }],
    tables: &[("dsp", &["nll_probe"])],
    audit: dispatch_core::manifest::Audit {
        areas: &[("nll_probe.item2.", dispatch_core::tenancy::api::audit::AuditArea::Collections)],
        ..dispatch_core::manifest::Audit::NONE
    },
    routes: || vec![
      dispatch_core::server::http::route::read("/custom-public", dispatch_core::server::http::route::Public, |_, _, _| Ok(dispatch_core::server::http::input::Reply::json(json!({"ok":true})))),
      dispatch_core::server::http::route::read("/custom-session", dispatch_core::server::http::route::Session, |_, _, _| Ok(dispatch_core::server::http::input::Reply::json(json!({"ok":true})))),
      dispatch_core::server::http::route::write("/custom-public-post", dispatch_core::server::http::route::Public, |_, _, _| Ok(dispatch_core::server::http::input::Reply::json(json!({"ran":true})))),
      dispatch_core::server::http::route::protocol(axum::http::Method::POST, "/custom-protocol", raw_callback),
      dispatch_core::server::http::route::read(
        "/api/dsp/nll-probe", dispatch_core::server::http::route::Dsp("nll_probe.view"),
        |db, c, _| Ok(dispatch_core::server::http::input::Reply::json(json!({"count": db.dsp(c.dsp_id())?.count("SELECT count(*) FROM nll_probe", [])?})))
    )],
    on: &[dispatch_core::on!(dispatch_core::manifest::events::PeopleChanged, custom_event)],
    dsp_maintenance: &[dispatch_core::manifest::DspMaintenance {
        every: Duration::from_secs(60),
        run: custom_upkeep,
    }],
    ..feature("nll_probe")
}.registered(Some("nll"), "nll_probe");
static FOREIGN: Feature = Feature {
    switch: dispatch_core::manifest::custom("sum_probe", "Foreign probe", &[]),
    page_sources: &[dispatch_core::manifest::page_source(
        "sum_probe.images",
        dispatch_core::manifest::Loads::Images,
        "https://foreign-media.example.test",
    )],
    permissions: &[dispatch_core::manifest::perm(
        "sum_probe.view",
        "Foreign permission",
        2,
    )],
    ..feature("sum_probe")
}
.registered(Some("sum"), "sum_probe");
fn raw_callback(
    state: Arc<State>,
    request: axum::extract::Request,
) -> dispatch_core::server::http::route::Served {
    Box::pin(async move {
        use axum::response::IntoResponse;
        let signature = request.headers().get("content-type").unwrap().clone();
        let body = axum::body::to_bytes(request.into_body(), 1024)
            .await
            .unwrap();
        match state
            .run(move |_| {
                Ok(dispatch_core::server::http::input::Reply::json(json!({
                    "body": String::from_utf8(body.to_vec()).unwrap(),
                    "contentType": signature.to_str().unwrap(),
                })))
            })
            .await
        {
            Ok(reply) => reply.into_response(),
            Err(error) => error.into_response(),
        }
    })
}
fn custom_upkeep(state: Arc<State>, dsp: String, due: bool) -> dispatch_core::manifest::Upkeep {
    Box::pin(async move {
        let at = dsp.clone();
        let result = state
            .run_serving(
                dsp.clone(),
                dispatch_core::server::cache::DataDomain::TENANT,
                move |db| db.dsp(&at)?.exec("INSERT INTO nll_probe VALUES (2)", []),
            )
            .await;
        MAINTAINED
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .send((dsp, due, result))
            .unwrap();
    })
}
fn custom_event(
    state: Arc<State>,
    dsp: String,
    _: dispatch_core::manifest::events::PeopleChanged,
) -> dispatch_core::manifest::Upkeep {
    Box::pin(async move {
        state
            .run(move |db| {
                db.dsp(&dsp)?.exec("INSERT INTO nll_probe VALUES (1)", [])?;
                Ok(())
            })
            .await
            .unwrap();
    })
}

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
        testing::install(&[], &[&PROBE, &CUSTOM, &FOREIGN]);
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let dashboard = root.path().join("dashboard");
        std::fs::create_dir_all(&dashboard).unwrap();
        std::fs::write(dashboard.join("index.html"), "<html><head></head></html>").unwrap();
        std::fs::create_dir(dashboard.join("assets")).unwrap();
        let mut private = serde_json::Map::new();
        for (name, body) in [
            ("custom-nll-entry.js", "export const label = 'Owner probe';"),
            ("custom-nll-lazy.js", "export const page = 'owner page';"),
            ("custom-nll-style.css", ".owner-probe { color: red; }"),
            ("custom-nll-image.svg", "<svg>owner image</svg>"),
        ] {
            std::fs::write(dashboard.join("assets").join(name), body).unwrap();
            let mut entry = json!({"owner":"nll"});
            if name == "custom-nll-entry.js" {
                entry["feature"] = json!("nll_probe");
                entry["styles"] = json!(["/assets/custom-nll-style.css"]);
            }
            private.insert(format!("/assets/{name}"), entry);
        }
        std::fs::write(
            dashboard.join("assets/shared.js"),
            "export const shared = true;",
        )
        .unwrap();
        std::fs::write(
            dashboard.join("assets/owner-runtime.js"),
            "export const admin = true;",
        )
        .unwrap();
        private.insert(
            "/assets/owner-runtime.js".into(),
            json!({"platform":true,"feature":"platform"}),
        );
        std::fs::write(
            dashboard.join("private-assets.json"),
            serde_json::to_vec(&private).unwrap(),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut config = Config::load().unwrap();
        config.root = root.path().join("state");
        db::private_dir(&config.root).unwrap();
        config.development = true;
        config.fixture = true;
        config.environment = Environment::Preview;
        config.standalone = true;
        config.dashboard = dashboard;
        config.port = port;
        config.origin = format!("http://127.0.0.1:{port}");
        config.invite_origin = format!("http://invite.localhost:{port}");
        config.dsp_origin = format!("http://{{code}}.localhost:{port}");
        let store = startup::prepare_storage(config.clone()).unwrap();
        operations::seed(&store).unwrap();
        let summit = store
            .platform
            .one("SELECT id FROM dsps WHERE code='sum'", [])
            .unwrap()
            .unwrap();
        testing::owner_of(&store, s(&summit, "id"), "summit@dispatch.test", "Summit").unwrap();
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
        // Startup must normalize this owner's off feature, but leave matching foreign
        // records alone, including a conflicting pair that would otherwise stop startup.
        for code in ["nll", "sum"] {
            let (id,): (String,) = store
                .platform
                .one_as("SELECT id FROM dsps WHERE code=?", [code])
                .unwrap()
                .unwrap();
            let data = store.dsp(&id).unwrap();
            data.set(
                "section:nll_next_probe.limits",
                &json!({"revision":7,"values":{"days":120},"at":"original","by":"original"}),
            )
            .unwrap();
            data.set("nll_next_probe.kept", &json!({"marker":code,"revision":4}))
                .unwrap();
            if code == "sum" {
                data.set("section:nll_probe.limits", &json!({"foreign":true}))
                    .unwrap();
                data.set("nll_probe.kept", &json!({"foreign":true}))
                    .unwrap();
            }
        }
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

#[tokio::test]
async fn custom_assets_require_the_owner_session_before_head_or_etag_handling() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let summit = server.dsp("sum").await;
    let owner = server.signed_in("member@dispatch.test", &north).await;
    let foreign = server.signed_in("summit@dispatch.test", &summit).await;
    let platform = server.signed_in("owner@dispatch.test", &summit).await;
    let anonymous = Who {
        cookie: String::new(),
        csrf: String::new(),
        view: String::new(),
        host: owner.host.clone(),
    };
    for who in [&owner, &foreign] {
        let session = server.send("GET", "/api/session", who, None).await;
        assert!(session.body.get("platformFrontend").is_none());
    }
    let session = server.send("GET", "/api/session", &platform, None).await;
    assert_eq!(session.body["platformFrontend"], "/assets/owner-runtime.js");
    // Both sessions are valid at their own host; metadata never includes the other's IDs,
    // even through a declared permission implication or parent.
    let off_view = server.open(&owner, &north).await;
    assert_eq!(off_view.body["custom"]["frontends"], json!([]));
    assert!(!off_view.body.to_string().contains("sum_probe"));
    assert_eq!(
        off_view.body["custom"]["permissions"][0]["implied"],
        json!([])
    );
    assert!(off_view.body["custom"]["permissions"][0]["under"].is_null());
    let own = north.clone();
    server
        .state
        .run(move |db| {
            let actor = testing::platform_owner(db);
            db.set_feature(&own, "nll_probe", true, &actor)?;
            Ok(())
        })
        .await
        .unwrap();
    let own_view = server.open(&owner, &north).await;
    assert_eq!(
        own_view.body["custom"]["frontends"][0]["url"],
        "/assets/custom-nll-entry.js"
    );
    let other_view = server.open(&foreign, &summit).await;
    assert_eq!(other_view.status, 200);
    assert!(!other_view.body.to_string().contains("nll_probe"));
    assert!(!other_view.body.to_string().contains("Owner probe"));
    let origin = format!("http://127.0.0.1:{}", server.port);
    let request = |method, path: &str, who: &Who| {
        server
            .client
            .request(method, format!("{origin}{path}"))
            .header(
                "host",
                who.host
                    .clone()
                    .unwrap_or_else(|| format!("127.0.0.1:{}", server.port)),
            )
            .header("cookie", &who.cookie)
    };
    for (who, own_sources, foreign_sources) in [
        (&anonymous, false, false),
        (&owner, true, false),
        (&foreign, false, true),
        (&platform, true, true),
    ] {
        let document = request(reqwest::Method::GET, "/", who)
            .send()
            .await
            .unwrap();
        assert_eq!(document.status(), 200);
        let csp = document.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(csp.contains("https://shared-media.example.test"));
        assert_eq!(
            csp.contains("https://owner-media.example.test"),
            own_sources
        );
        assert_eq!(
            csp.contains("https://owner-calls.example.test"),
            own_sources
        );
        assert_eq!(
            csp.contains("https://foreign-media.example.test"),
            foreign_sources
        );
    }
    for path in [
        "/assets/custom-nll-entry.js",
        "/assets/custom-nll-lazy.js",
        "/assets/custom-nll-style.css",
        "/assets/custom-nll-image.svg",
    ] {
        let allowed = request(reqwest::Method::GET, path, &owner)
            .send()
            .await
            .unwrap();
        assert_eq!(allowed.status(), 200, "{path}");
        assert_eq!(allowed.headers()["cache-control"], "private, no-store");
        let tag = allowed.headers()["etag"].clone();
        assert!(!allowed.bytes().await.unwrap().is_empty());
        for method in [reqwest::Method::GET, reqwest::Method::HEAD] {
            for who in [&foreign, &anonymous] {
                let refused = request(method.clone(), path, who)
                    .header("if-none-match", &tag)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(refused.status(), 404, "{path}");
                assert!(refused.headers().get("etag").is_none());
            }
            for who in [&owner, &platform] {
                assert_eq!(
                    request(method.clone(), path, who)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    200,
                    "{path}"
                );
                assert_eq!(
                    request(method.clone(), path, who)
                        .header("if-none-match", &tag)
                        .send()
                        .await
                        .unwrap()
                        .status(),
                    304,
                    "{path}"
                );
            }
        }
    }
    for who in [&owner, &foreign, &anonymous] {
        assert_eq!(
            request(reqwest::Method::GET, "/assets/owner-runtime.js", who)
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            request(reqwest::Method::GET, "/assets/shared.js", who)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            request(reqwest::Method::GET, "/private-assets.json", who)
                .send()
                .await
                .unwrap()
                .status(),
            404
        );
    }
    assert_eq!(
        request(reqwest::Method::GET, "/assets/owner-runtime.js", &platform)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    // A session awaiting MFA gets neither the private entry nor its CSP origins, even
    // on /api/session, which itself remains readable to complete verification.
    server
        .state
        .run(|db| {
            let actor = testing::platform_owner(db);
            db.directory(None)?.exec(
                "INSERT INTO authenticator_apps(user_id,secret,created_at) VALUES (?,'sealed',1)",
                [&actor],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let gated = request(reqwest::Method::GET, "/api/session", &platform)
        .send()
        .await
        .unwrap();
    assert_eq!(gated.status(), 200);
    assert!(
        !gated.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("https://owner-media.example.test")
    );
    let gated: Value = gated.json().await.unwrap();
    assert_eq!(gated["security"]["required"], true);
    assert_eq!(gated["security"]["verified"], false);
    assert!(gated.get("platformFrontend").is_none());
    assert_eq!(
        request(reqwest::Method::GET, "/assets/owner-runtime.js", &platform)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let hash = crypto::sha(
        platform
            .cookie
            .strip_prefix("dispatch_session=")
            .unwrap()
            .as_bytes(),
    );
    server
        .state
        .run(move |db| {
            db.directory(None)?.exec(
                "INSERT INTO session_security(session_hash,verified_at) VALUES (?,?)",
                rusqlite::params![hash, db::now()],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    let verified = server.send("GET", "/api/session", &platform, None).await;
    assert_eq!(
        verified.body["platformFrontend"],
        "/assets/owner-runtime.js"
    );
    assert_eq!(
        request(reqwest::Method::GET, "/assets/owner-runtime.js", &platform)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    server
        .state
        .run(move |db| testing::set_dsp_status(db, &north, "suspended"))
        .await
        .unwrap();
    assert_eq!(
        request(reqwest::Method::GET, "/assets/custom-nll-entry.js", &owner)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
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

#[tokio::test]
async fn custom_storage_switches_events_and_routes_belong_only_to_the_owner_code() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let summit = server.dsp("sum").await;
    let (own, other) = (north.clone(), summit.clone());
    server.state.run(move |db| {
        let actor = testing::platform_owner(db);
        // Assigning the code after provisioning applies the custom migration immediately.
        assert!(db.dsp(&own)?.has_table("nll_probe")?);
        assert!(!db.dsp(&other)?.has_table("nll_probe")?);
        assert!(!db.feature_enabled(&own, "nll_probe")?);
        assert_eq!(db.dsp(&own)?.setting("section:nll_probe.limits", Value::Null)?["at"], "original");
        assert_eq!(dispatch_core::settings::read_key(db, &own, &CUSTOM.settings_keys[0], Value::Null)?["marker"], "nll");
        assert_eq!(db.dsp(&other)?.setting("section:nll_probe.limits", Value::Null)?, json!({"foreign":true}));
        assert_eq!(db.dsp(&other)?.setting("section:nll_next_probe.limits", Value::Null)?["revision"], 7);
        assert_eq!(db.dsp(&other)?.setting("nll_next_probe.kept", Value::Null)?["marker"], "sum");
        let report = db.feature_report(&own)?;
        assert!(report.features.iter().any(|feature| feature.feature == "nll_probe" && !feature.enabled));
        assert!(!report.features.iter().any(|feature| feature.feature == "sum_probe"));
        let custom = report.custom.unwrap();
        assert_eq!(custom.len(), 1);
        assert_eq!((custom[0].id.as_str(), custom[0].label.as_str()), ("nll_probe", "Owner probe"));
        let other_report = db.feature_report(&other)?;
        assert!(!other_report.features.iter().any(|feature| feature.feature == "nll_probe"));
        assert_eq!(other_report.custom.unwrap()[0].id, "sum_probe");
        let directory = db.dsp(&own)?;
        directory.exec("DELETE FROM owner_migrations WHERE owner='nll_probe'", [])?;
        directory.exec("DROP TABLE nll_probe", [])?;
        directory.transaction(|| {
            db.set_code(&own, "nll")?;
            assert!(db.dsp(&own)?.has_table("nll_probe")?);
            Ok(())
        })?;
        assert_eq!(db.set_code(&own, "othercode").unwrap_err().code, "custom_feature_transfer_required");
        assert_eq!(db.set_feature(&other, "nll_probe", true, &actor).unwrap_err().status, 404);
        assert_eq!(db.set_visibility(&other, "nll_probe", dispatch_core::tenancy::visibility::Visibility::Shown, &actor).unwrap_err().status, 404);
        // A stale or imported switch row cannot bypass runtime ownership.
        db.platform.exec("INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,'nll_probe',1,'')", [&other])?;
        db.enable_all_features(&other)?;
        db.keep_switches()?;
        assert!(!db.feature_enabled(&other, "nll_probe")?);
        assert!(!dispatch_core::tenancy::catalog::grants(&db.features(&other)?, "nll_probe.view"));
        db.set_feature(&own, "nll_probe", true, &actor)?;
        Ok(())
    }).await.unwrap();
    dispatch_core::manifest::events::publish(
        &server.state,
        &north,
        dispatch_core::manifest::events::PeopleChanged,
    )
    .await;
    dispatch_core::manifest::events::publish(
        &server.state,
        &summit,
        dispatch_core::manifest::events::PeopleChanged,
    )
    .await;
    let owner = server.signed_in("owner@dispatch.test", &north).await;
    let other = server.signed_in("owner@dispatch.test", &summit).await;
    assert_eq!(
        server.days(&owner, "nll_probe.limits").await,
        Some((json!(120), 7))
    );
    let body = json!({"values":{"days":180},"revision":7});
    let saved = server
        .send(
            "POST",
            "/api/dsp/settings/nll_next_probe.limits",
            &owner,
            Some(body.clone()),
        )
        .await;
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.body["section"]["id"], "nll_probe.limits");
    assert_eq!(saved.body["section"]["revision"], 8);
    assert_eq!(
        server
            .send(
                "POST",
                "/api/dsp/settings/nll_next_probe.limits",
                &other,
                Some(body)
            )
            .await
            .status,
        404
    );
    let answer = server.send("GET", "/api/dsp/nll-probe", &owner, None).await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    assert_eq!(answer.body["count"], 1);
    assert_eq!(
        server
            .send("GET", "/api/dsp/nll-probe", &other, None)
            .await
            .status,
        404
    );
    let at_other = Who {
        host: Some(format!("sum.localhost:{}", server.port)),
        ..owner
    };
    assert_eq!(
        server
            .send("GET", "/api/dsp/nll-probe", &at_other, None)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn scheduled_custom_upkeep_receives_only_its_owner_even_with_foreign_switch_rows() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let summit = server.dsp("sum").await;
    let own = north.clone();
    server.state.run(move |db| {
        let actor = testing::platform_owner(db);
        // The owner sorts last: receiving its callback means this declaration already
        // passed every non-owner, including one with an imported enabled switch row.
        db.platform.exec("UPDATE dsps SET name='ZZ owner' WHERE id=?", [&own])?;
        db.platform.exec("INSERT INTO dsp_features(dsp_id,feature,enabled,changed_at) VALUES (?,'nll_probe',1,'')", [&summit])?;
        db.set_feature(&own, "nll_probe", true, &actor)?;
        Ok(())
    }).await.unwrap();
    let (sent, mut received) = tokio::sync::mpsc::unbounded_channel();
    *MAINTAINED.lock().unwrap() = Some(sent);
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let mut scheduler = tokio::spawn(dispatch_core::collection::jobs::start(
        server.state.clone(),
        stopped,
    ));
    let (dsp, due, wrote) = tokio::select! {
        found = received.recv() => found.unwrap(),
        ended = &mut scheduler => panic!("scheduler ended before upkeep: {ended:?}"),
    };
    assert_eq!(dsp, north);
    assert!(due);
    assert_eq!(wrote.unwrap(), 1);
    stop.send_replace(true);
    scheduler.await.unwrap().unwrap();
    assert!(
        received.try_recv().is_err(),
        "no other DSP received custom upkeep"
    );
    *MAINTAINED.lock().unwrap() = None;
}

#[tokio::test]
async fn a_custom_route_keeps_its_declared_public_or_session_grant() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let anonymous = Who {
        cookie: String::new(),
        csrf: String::new(),
        view: String::new(),
        host: Some(format!("nll.localhost:{}", server.port)),
    };
    assert_eq!(
        server
            .send("GET", "/custom-public", &anonymous, None)
            .await
            .status,
        200
    );
    assert_eq!(
        server
            .send("GET", "/custom-session", &anonymous, None)
            .await
            .status,
        401
    );
    let mut member = server.signed_in("member@dispatch.test", &north).await;
    member.view.clear();
    assert_eq!(
        server
            .send("GET", "/custom-session", &member, None)
            .await
            .status,
        200
    );
    let raw = server
        .send(
            "POST",
            "/custom-protocol",
            &anonymous,
            Some(json!({"event":"hello"})),
        )
        .await;
    assert_eq!(raw.status, 200);
    assert_eq!(raw.body["body"], r#"{"event":"hello"}"#);
    assert_eq!(raw.body["contentType"], "application/json");
    assert_eq!(
        server
            .send("POST", "/shared-protocol", &anonymous, Some(json!({})))
            .await
            .status,
        404
    );
    let admin = Who {
        host: None,
        cookie: String::new(),
        csrf: String::new(),
        view: String::new(),
    };
    assert_eq!(
        server
            .send("POST", "/shared-protocol", &admin, Some(json!({})))
            .await
            .status,
        200
    );
    let other = Who {
        host: Some(format!("sum.localhost:{}", server.port)),
        ..anonymous
    };
    assert_eq!(
        server
            .send("POST", "/custom-protocol", &other, Some(json!({})))
            .await
            .status,
        404
    );
    assert_eq!(
        server
            .send("GET", "/custom-public", &other, None)
            .await
            .status,
        404
    );
    assert_eq!(
        server
            .send("GET", "/custom-session", &other, None)
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn custom_callbacks_have_only_a_live_owning_dsp_entrypoint() {
    let server = Server::start().await;
    let north = server.dsp("nll").await;
    let member = server.signed_in("member@dispatch.test", &north).await;
    let mut answers = Vec::new();
    for host in [None, Some(format!("invite.localhost:{}", server.port))] {
        let visitor = Who {
            cookie: String::new(),
            csrf: String::new(),
            view: String::new(),
            host,
        };
        for (method, path) in [
            ("GET", "/custom-public"),
            ("GET", "/custom-session"),
            ("POST", "/custom-protocol"),
        ] {
            answers.push((
                "outside owner host",
                path,
                server
                    .send(method, path, &visitor, Some(json!({})))
                    .await
                    .status,
            ));
        }
    }
    let own = north.clone();
    server
        .state
        .run(move |db| testing::set_dsp_status(db, &own, "suspended"))
        .await
        .unwrap();
    for (method, path) in [
        ("GET", "/custom-public"),
        ("GET", "/custom-session"),
        ("POST", "/custom-protocol"),
    ] {
        answers.push((
            "disabled owner",
            path,
            server
                .send(method, path, &member, Some(json!({})))
                .await
                .status,
        ));
    }
    server
        .state
        .run(move |db| {
            let actor = testing::platform_owner(db);
            let name = db.find_dsp(&north)?.name;
            db.delete_dsp(&north, &name, &actor)
        })
        .await
        .unwrap();
    for (method, path) in [
        ("GET", "/custom-public"),
        ("GET", "/custom-session"),
        ("POST", "/custom-protocol"),
    ] {
        answers.push((
            "deleted owner",
            path,
            server
                .send(method, path, &member, Some(json!({})))
                .await
                .status,
        ));
    }
    assert!(
        answers.iter().all(|(_, _, status)| *status == 404),
        "{answers:?}"
    );
}

#[tokio::test]
async fn a_custom_public_callback_checks_the_owner_again_after_its_body_arrives() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for path in ["/custom-public-post", "/custom-protocol"] {
        let server = Server::start().await;
        let north = server.dsp("nll").await;
        let mut connection = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
            .await
            .unwrap();
        let headers = format!(
            "POST {path} HTTP/1.1\r\nHost: nll.localhost:{}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
            server.port
        );
        connection.write_all(headers.as_bytes()).await.unwrap();
        // Hyper asks for the body only after the route's initial owner admission passed.
        let mut ready = [0; b"HTTP/1.1 100 Continue\r\n\r\n".len()];
        connection.read_exact(&mut ready).await.unwrap();
        assert_eq!(&ready, b"HTTP/1.1 100 Continue\r\n\r\n");
        server
            .state
            .run(move |db| testing::set_dsp_status(db, &north, "suspended"))
            .await
            .unwrap();
        connection.write_all(b"{}").await.unwrap();
        let mut answer = String::new();
        connection.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 404"), "{answer}");
    }
}
