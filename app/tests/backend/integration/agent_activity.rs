//! The Agents page's Activity log, over HTTP against the real router: each REST and MCP call
//! a key or connected app makes is held in memory and written down in batches, never one by
//! one; refusals keep their code; the log pages newest first, narrows to one key or to what
//! was refused, and forgets calls past 90 days.
use dispatch_core::{
    State,
    db::{self, Store, s},
    foundation::{config::Config, crypto},
    mcp::{
        activity,
        api::types::{AgentActivityKey, AgentKeyKind, AgentKeyRequest},
        oauth,
    },
    server::operations,
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, sync::Arc};

struct Server {
    _root: tempfile::TempDir,
    origin: String,
    state: Arc<State>,
    client: reqwest::Client,
}
struct Answer {
    status: u16,
    body: Value,
    bytes: usize,
}
impl Server {
    /// The router alone, without the scheduler, so nothing is written down unless a test
    /// flushes it.
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
            _root: root,
            origin: format!("http://127.0.0.1:{port}"),
            state,
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
        }
    }
    async fn send(&self, request: reqwest::RequestBuilder) -> Answer {
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap();
        Answer {
            status,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            bytes: bytes.len(),
        }
    }
    /// The platform owner's session cookie, written directly so tests skip password hashing.
    async fn owner(&self) -> String {
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
        format!("dispatch_session={raw}")
    }
    async fn log(&self, cookie: &str, query: &str) -> Answer {
        self.send(
            self.client
                .get(format!(
                    "{}/api/platform/agents/activity{query}",
                    self.origin
                ))
                .header("cookie", cookie),
        )
        .await
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
    /// A key made the way the Agents page makes one, reaching `dsps` (every DSP when empty).
    async fn key(&self, name: &str, dsps: Vec<String>) -> (String, String) {
        let reads = json!({"areas":["routes","timecards","meal_breaks","dvic","feedback",
            "safety","returns","scorecard"],"bypass":false});
        self.key_reading(name, dsps, reads).await
    }
    /// The same, reading as `reads` says at every DSP.
    async fn key_reading(&self, name: &str, dsps: Vec<String>, reads: Value) -> (String, String) {
        let request = AgentKeyRequest::parse(&json!({"name":name,"allDsps":dsps.is_empty(),
            "dsps":dsps,"access":"read","reads":reads,"dspReads":[],"expiresAt":null}))
        .unwrap();
        self.state
            .run(move |db| {
                let owner = db
                    .platform
                    .one("SELECT id FROM users WHERE email='owner@dispatch.test'", [])?
                    .unwrap();
                let made = db.create_agent_key(s(&owner, "id"), &request)?;
                Ok((made.key.id, made.token))
            })
            .await
            .unwrap()
    }
    /// A connected app reaching every DSP and its access token, as approving it leaves them.
    async fn app(&self, name: &'static str) -> (String, String) {
        let token = access_token();
        let hash = crypto::sha(&token);
        let resource = oauth::resource(&self.state.config);
        let id = crypto::id("agentkey").unwrap();
        let key = id.clone();
        self.state
            .run(move |db| {
                let owner = db
                    .platform
                    .one("SELECT id FROM users WHERE email='owner@dispatch.test'", [])?
                    .unwrap();
                db.platform.exec(
                    "INSERT INTO agent_keys(id,name,hash,hint,user_id,all_dsps,access,tools,\
                     locations,created_at,kind,client_id,client_name,client_verified) VALUES \
                     (?1,?2,'app:'||?1,'',?3,1,'read','full',0,?4,'app','https://claude.ai/x',\
                     'Claude Code',1)",
                    rusqlite::params![key, name, s(&owner, "id"), db::iso()],
                )?;
                db.platform.exec(
                    "INSERT INTO oauth_tokens(hash,key_id,kind,resource,created_at,expires_at) \
                     VALUES (?,?,'access',?,?,?)",
                    rusqlite::params![
                        hash,
                        key,
                        resource,
                        db::iso(),
                        db::at(db::now() + 3_600_000)
                    ],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        (id, token)
    }
    async fn rest(&self, token: &str, path: &str) -> Answer {
        self.send(
            self.client
                .get(format!("{}{path}", self.origin))
                .header("authorization", format!("Bearer {token}")),
        )
        .await
    }
    async fn mcp(&self, token: &str, method: &str, params: Value) -> Answer {
        self.send(
            self.client
                .post(format!("{}/api/v1/mcp", self.origin))
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-06-18")
                .body(json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string()),
        )
        .await
    }
    async fn tool(&self, token: &str, name: &str, arguments: Value) -> Answer {
        self.mcp(
            token,
            "tools/call",
            json!({"name":name,"arguments":arguments}),
        )
        .await
    }
    async fn written(&self) -> i64 {
        self.state
            .read(|db| db.platform.count("SELECT count(*) FROM agent_activity", []))
            .await
            .unwrap()
    }
}

/// A connected app's access token: `dsa_dev_`, 32 letters and digits, and their checksum.
fn access_token() -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let random = crypto::random::<32>().unwrap();
    let body: String = random
        .iter()
        .map(|byte| ALPHABET[usize::from(*byte) % 62] as char)
        .collect();
    let head = format!("dsa_dev_{body}");
    let mut crc = !0u32;
    for &byte in head.as_bytes() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    let mut value = !crc;
    let mut check = [b'0'; 6];
    for slot in check.iter_mut().rev() {
        *slot = ALPHABET[(value % 62) as usize];
        value /= 62;
    }
    head + std::str::from_utf8(&check).unwrap()
}

/// What the log says of each call, oldest first: surface, kind, DSP and outcome.
fn seen(rows: &Value) -> Vec<(String, String, String, String)> {
    let mut seen: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                s(row, "surface").to_owned(),
                s(&row["key"], "kind").to_owned(),
                row["dsp"]["name"].as_str().unwrap_or("-").to_owned(),
                s(row, "outcome").to_owned(),
            )
        })
        .collect();
    seen.reverse();
    seen
}
fn row(surface: &str, kind: &str, dsp: &str, outcome: &str) -> (String, String, String, String) {
    (surface.into(), kind.into(), dsp.into(), outcome.into())
}

#[tokio::test]
async fn rest_and_mcp_calls_are_held_then_written_with_their_dsp_and_outcome() {
    let server = Server::start().await;
    let cookie = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let (key, token) = server
        .key("Laptop – Claude Code", vec![north.clone()])
        .await;
    let (app, access) = server.app("Claude Code").await;

    // A key reaching one DSP: its answers are about that DSP, unless it names another.
    let whoami = server.rest(&token, "/api/v1/whoami").await;
    assert_eq!(whoami.status, 200, "{}", whoami.body);
    assert_eq!(server.rest(&token, "/api/v1/status").await.status, 200);
    let elsewhere = server.rest(&token, "/api/v1/status?dsp=Nowhere").await;
    assert_eq!(elsewhere.body["error"], "dsp_not_found");
    // The MCP handshake and lists are no calls; a tool call is, refused or not.
    let hello = json!({"protocolVersion":"2025-06-18","capabilities":{},
        "clientInfo":{"name":"claude-code","version":"2"}});
    assert_eq!(server.mcp(&token, "initialize", hello).await.status, 200);
    assert_eq!(
        server.mcp(&token, "tools/list", json!({})).await.status,
        200
    );
    let found = server.tool(&token, "find_drivers", json!({"q":"a"})).await;
    assert_eq!(found.body["result"]["isError"], false, "{}", found.body);
    let bare = server.tool(&token, "driver_report", json!({})).await;
    assert_eq!(bare.body["result"]["isError"], true);
    let invented = server.tool(&token, "secret Avery Morgan", json!({})).await;
    assert_eq!(invented.body["result"]["isError"], true);
    // A connected app reaching every DSP must name one.
    assert_eq!(server.rest(&access, "/api/v1/whoami").await.status, 200);
    let unclear = server.tool(&access, "dvic_inspections", json!({})).await;
    assert!(
        unclear.body["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("dsp_required"),
        "{}",
        unclear.body
    );
    let named = server
        .tool(&access, "data_status", json!({"dsp":"Summit Delivery"}))
        .await;
    assert_eq!(named.body["result"]["isError"], false, "{}", named.body);
    // A request no key signs is no one's call.
    assert_eq!(
        server.rest("dsk_dev_nope", "/api/v1/whoami").await.status,
        401
    );

    // Nothing was written yet: every call waits in memory.
    assert_eq!(server.written().await, 0);
    assert_eq!(server.state.activity.pending(), 9);
    assert_eq!(server.log(&cookie, "").await.body["rows"], json!([]));
    assert_eq!(activity::flush(&server.state).await.unwrap(), 9);
    assert_eq!(server.state.activity.pending(), 0);
    assert_eq!(server.written().await, 9);

    let log = server.log(&cookie, "").await;
    assert_eq!(log.status, 200, "{}", log.body);
    assert_eq!(log.body["next"], Value::Null);
    assert_eq!(
        seen(&log.body["rows"]),
        [
            row("rest:whoami", "key", "-", "ok"),
            row("rest:status", "key", "Northline Logistics", "ok"),
            row("rest:status", "key", "-", "dsp_not_found"),
            row("mcp:find_drivers", "key", "Northline Logistics", "ok"),
            row(
                "mcp:driver_report",
                "key",
                "Northline Logistics",
                "missing_parameter"
            ),
            row("mcp:unknown", "key", "-", "unknown_tool"),
            row("rest:whoami", "app", "-", "ok"),
            row("mcp:dvic_inspections", "app", "-", "dsp_required"),
            row("mcp:data_status", "app", "Summit Delivery", "ok"),
        ]
    );
    let rows = log.body["rows"].as_array().unwrap();
    let first = rows.last().unwrap();
    assert_eq!(
        first["key"],
        json!({"id":key,"name":"Laptop – Claude Code","kind":"key"})
    );
    assert_eq!(rows[0]["key"]["id"], app.as_str());
    assert_eq!(first["dsp"], Value::Null);
    // How much it answered is the body the agent read; how long it took, in milliseconds.
    assert_eq!(first["bytes"], whoami.bytes);
    for row in rows {
        assert!(row["bytes"].as_u64().unwrap() > 0, "{row}");
    }
    assert!(first["ms"].as_u64().unwrap() < 60_000);
    assert!(first["at"].as_str().unwrap().ends_with('Z'));
    let northline = &rows[rows.len() - 2];
    assert_eq!(northline["dsp"]["id"], north.as_str());
    // Nothing the agent asked is kept beyond the endpoint or tool, nor any token.
    let text = log.body.to_string();
    for secret in ["Nowhere", "Avery", "secret", &token, &access, "\"q\""] {
        assert!(!text.contains(secret), "{secret} in {text}");
    }
}

#[tokio::test]
async fn a_key_refused_for_its_rate_is_recorded_once_a_minute() {
    let server = Server::start().await;
    let cookie = server.owner().await;
    let (key, token) = server.key("Busy", vec![]).await;
    // The minute's calls already made, so the next ones are refused, over REST and MCP
    // alike. A minute may turn between filling it and calling: then fill the new one.
    let mut minutes = 0;
    for _ in 0..3 {
        while server.state.agents.admit(&key, "curl").is_ok() {}
        let rest = server.rest(&token, "/api/v1/whoami").await;
        let tool = server.tool(&token, "whoami", json!({})).await;
        if rest.status == 429 {
            assert_eq!(rest.body["error"], "rate_limited");
            minutes += 1;
        }
        if rest.status == 429 && tool.status == 429 {
            break;
        }
    }
    assert!(minutes > 0);
    activity::flush(&server.state).await.unwrap();
    let log = server.log(&cookie, "?outcome=refused").await;
    let refused: Vec<_> = seen(&log.body["rows"])
        .into_iter()
        .filter(|(_, _, _, outcome)| outcome == "rate_limited")
        .collect();
    // Refused twice in a minute, recorded once: the first.
    assert_eq!(refused.len(), minutes, "{}", log.body);
    assert_eq!(refused.last().unwrap().0, "rest:whoami");
}

#[tokio::test]
async fn the_log_pages_newest_first_and_narrows_to_a_key_or_to_refusals() {
    let server = Server::start().await;
    let cookie = server.owner().await;
    let key = |id: &str| AgentActivityKey {
        id: id.into(),
        name: format!("Key {id}"),
        kind: AgentKeyKind::Key,
    };
    // Calls as the buffer holds them, some starting at the same moment.
    let now = db::now();
    let calls: Vec<activity::Call> = (0..7)
        .map(|n| activity::Call {
            at: now - 60_000 + (n / 2) * 1000,
            key: key(if n % 3 == 0 {
                "agentkey_a"
            } else {
                "agentkey_b"
            }),
            surface: format!("rest:call{n}"),
            dsp: None,
            outcome: if n % 2 == 0 { "ok" } else { "unknown_metric" }.into(),
            bypassed: false,
            ms: 1,
            bytes: 2,
        })
        .collect();
    for call in &calls {
        server.state.activity.record(call.clone());
    }
    activity::flush(&server.state).await.unwrap();
    let surfaces = |answer: &Answer| -> Vec<String> {
        answer.body["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| s(row, "surface").to_owned())
            .collect()
    };
    // Newest first; the same moment, the later call first.
    let all: Vec<String> = (0..7).rev().map(|n| format!("rest:call{n}")).collect();
    let mut paged = vec![];
    let mut query = "?limit=3".to_owned();
    loop {
        let page = server.log(&cookie, &query).await;
        assert_eq!(page.status, 200, "{}", page.body);
        assert!(page.body["rows"].as_array().unwrap().len() <= 3);
        paged.extend(surfaces(&page));
        match page.body["next"].as_str() {
            Some(next) => query = format!("?limit=3&before={next}"),
            None => break,
        }
    }
    assert_eq!(paged, all);
    let only = |n: &[usize]| -> Vec<String> {
        all.iter()
            .filter(|s| n.iter().any(|n| s.ends_with(&n.to_string())))
            .cloned()
            .collect()
    };
    assert_eq!(
        surfaces(&server.log(&cookie, "?key=agentkey_a").await),
        only(&[0, 3, 6])
    );
    assert_eq!(
        surfaces(&server.log(&cookie, "?outcome=refused").await),
        only(&[1, 3, 5])
    );
    assert_eq!(
        surfaces(&server.log(&cookie, "?outcome=ok").await),
        only(&[0, 2, 4, 6])
    );
    let narrowed = server
        .log(&cookie, "?key=agentkey_b&outcome=refused&limit=1")
        .await;
    assert_eq!(surfaces(&narrowed), ["rest:call5"]);
    let next = narrowed.body["next"].as_str().unwrap().to_owned();
    let after = server
        .log(
            &cookie,
            &format!("?key=agentkey_b&outcome=refused&before={next}"),
        )
        .await;
    assert_eq!(surfaces(&after), ["rest:call1"]);
    assert_eq!(after.body["next"], Value::Null);
    for wrong in [
        "?outcome=failed",
        "?before=yesterday",
        "?limit=0",
        "?limit=201",
        "?dsp=x",
    ] {
        assert_eq!(server.log(&cookie, wrong).await.status, 400, "{wrong}");
    }
    // Only a platform owner reads it, never an agent.
    let (_, token) = server.key("Reader", vec![]).await;
    assert_ne!(
        server
            .rest(&token, "/api/platform/agents/activity")
            .await
            .status,
        200
    );
}

#[tokio::test]
async fn a_key_past_its_days_calls_is_capped_and_a_restart_keeps_the_cap() {
    let server = Server::start().await;
    let cookie = server.owner().await;
    let (busy, _) = server.key("Busy", vec![]).await;
    let (quiet, _) = server.key("Quiet", vec![]).await;
    let day = 86_400_000;
    let midnight = db::now() / day * day;
    let call = |key: &str, at: i64| activity::Call {
        at,
        key: AgentActivityKey {
            id: key.into(),
            name: "Key".into(),
            kind: AgentKeyKind::Key,
        },
        surface: "rest:whoami".into(),
        dsp: None,
        outcome: "ok".into(),
        bypassed: false,
        ms: 1,
        bytes: 1,
    };
    // All but one of today's calls, as the server wrote them before it restarted, and
    // yesterday's, which count toward no cap of today's.
    let earlier: Vec<activity::Call> = (0..i64::from(activity::DAILY) - 1)
        .map(|_| call(&busy, midnight))
        .chain((0..5).map(|_| call(&busy, midnight - 1000)))
        .collect();
    server
        .state
        .run(move |db| db.record_agent_activity(&earlier))
        .await
        .unwrap();
    let restarted = State::new(server.state.config.clone()).unwrap();
    let now = db::now();
    if now / day * day != midnight {
        return; // The day turned while the test ran.
    }
    for n in 0..3 {
        restarted.activity.record(call(&busy, now + n));
    }
    restarted.activity.record(call(&quiet, now + 3));
    // The day's last call, then the row that marks it capped; the third is only counted.
    assert_eq!(activity::flush(&restarted).await.unwrap(), 3);
    let counted = busy.clone();
    let today = server
        .state
        .read(move |db| {
            db.platform.count(
                "SELECT count(*) FROM agent_activity WHERE key_id=? AND at>=?",
                rusqlite::params![counted, midnight],
            )
        })
        .await
        .unwrap();
    assert_eq!(today, i64::from(activity::DAILY) + 1);

    let listed = |answer: Answer| -> Vec<(String, String, String)> {
        assert_eq!(answer.status, 200, "{}", answer.body);
        answer.body["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    s(row, "surface").to_owned(),
                    s(row, "outcome").to_owned(),
                    s(&row["key"], "name").to_owned(),
                )
            })
            .collect()
    };
    let row = |surface: &str, outcome: &str, name: &str| {
        (surface.to_owned(), outcome.to_owned(), name.to_owned())
    };
    let marker = row("activity:capped", "capped", "Busy");
    let last = row("rest:whoami", "ok", "Busy");
    assert_eq!(
        listed(server.log(&cookie, "?limit=3").await),
        [
            row("rest:whoami", "ok", "Quiet"),
            marker.clone(),
            last.clone()
        ]
    );
    // The marker stands for calls that ended every way, so either filter lists it.
    let refused = listed(server.log(&cookie, "?outcome=refused").await);
    assert_eq!(refused, std::slice::from_ref(&marker));
    let only = format!("?key={busy}&outcome=ok&limit=2");
    assert_eq!(listed(server.log(&cookie, &only).await), [marker, last]);
    let capped = &server.log(&cookie, "?outcome=refused").await.body["rows"][0];
    assert_eq!(s(capped, "at"), db::at(now + 1));
    assert_eq!((&capped["ms"], &capped["bytes"]), (&json!(0), &json!(0)));
    assert_eq!(capped["dsp"], Value::Null);
}

#[tokio::test]
async fn calls_past_ninety_days_are_forgotten() {
    let server = Server::start().await;
    let day = 86_400_000;
    let call = |age: i64| activity::Call {
        at: db::now() - age,
        key: AgentActivityKey {
            id: "agentkey_a".into(),
            name: "Laptop".into(),
            kind: AgentKeyKind::Key,
        },
        surface: format!("rest:{}", age / day),
        dsp: None,
        outcome: "ok".into(),
        bypassed: false,
        ms: 1,
        bytes: 1,
    };
    let calls = vec![
        call(91 * day),
        call(90 * day + 60_000),
        call(89 * day),
        call(0),
    ];
    let kept = server
        .state
        .run(move |db| {
            db.record_agent_activity(&calls)?;
            let gone = db.prune_agent_activity()?;
            let kept = db
                .platform
                .query_as::<(String,)>("SELECT surface FROM agent_activity ORDER BY at", [])?;
            Ok((gone, kept))
        })
        .await
        .unwrap();
    assert_eq!(kept.0, 2);
    assert_eq!(kept.1, [("rest:89".to_owned(),), ("rest:0".to_owned(),)]);
}

#[tokio::test]
async fn reading_the_log_reads_one_index_in_order() {
    let server = Server::start().await;
    let plans = server
        .state
        .read(|db| {
            let mut plans = vec![];
            for filter in [
                "",
                " AND a.key_id='k'",
                " AND a.outcome<>'ok'",
                " AND (a.at,a.id)<(1,2)",
            ] {
                let plan = db.platform.all(
                    &format!(
                        "EXPLAIN QUERY PLAN SELECT a.id FROM agent_activity a LEFT JOIN \
                         agent_keys k ON k.id=a.key_id LEFT JOIN dsps d ON d.id=a.dsp_id \
                         WHERE 1{filter} ORDER BY a.at DESC,a.id DESC LIMIT 51"
                    ),
                    [],
                )?;
                plans.push(
                    plan.iter()
                        .map(|row| s(row, "detail").to_owned())
                        .collect::<Vec<_>>()
                        .join("; "),
                );
            }
            Ok(plans)
        })
        .await
        .unwrap();
    for plan in &plans {
        assert!(plan.contains("USING INDEX agent_activity_"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    assert!(plans[1].contains("agent_activity_key"), "{}", plans[1]);
    assert!(plans[2].contains("agent_activity_refused"), "{}", plans[2]);
}

#[tokio::test]
async fn a_call_that_read_a_switched_off_feature_by_bypassing_it_is_marked() {
    let server = Server::start().await;
    let cookie = server.owner().await;
    let north = server.dsp("Northline Logistics").await;
    let reads = json!({"areas":["timecards","dvic"],"bypass":true});
    let (key, token) = server
        .key_reading("Nightly report", vec![north.clone()], reads)
        .await;
    // Northline switches DVIC off; the key reads it anyway, and the answer says so.
    server
        .state
        .run(move |db| {
            let (owner,): (String,) = db
                .platform
                .one_as("SELECT id FROM users WHERE email='owner@dispatch.test'", [])?
                .unwrap();
            db.set_feature(&north, "dvic", false, &owner).map(|_| ())
        })
        .await
        .unwrap();
    let rest = server.rest(&token, "/api/v1/dvic").await;
    assert_eq!(rest.status, 200, "{}", rest.body);
    assert_eq!(rest.body["bypassed"], json!(["DVIC"]));
    let tool = server.tool(&token, "dvic_inspections", json!({})).await;
    let text = tool.body["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap()["bypassed"],
        json!(["DVIC"])
    );
    // Timecards are on: read as ever, unmarked. A refusal bypasses nothing.
    let cards = server.rest(&token, "/api/v1/timecards").await;
    assert_eq!(cards.status, 200, "{}", cards.body);
    assert!(cards.body.get("bypassed").is_none());
    let routes = server.rest(&token, "/api/v1/routes").await;
    assert_eq!(routes.body["error"], "not_allowed");
    activity::flush(&server.state).await.unwrap();
    let log = server.log(&cookie, &format!("?key={key}")).await;
    let mut marked: Vec<(String, bool)> = log.body["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (s(row, "surface").to_owned(), row["bypassed"] == true))
        .collect();
    marked.reverse();
    let expected: Vec<(String, bool)> = [
        ("rest:dvic", true),
        ("mcp:dvic_inspections", true),
        ("rest:timecards", false),
        ("rest:routes", false),
    ]
    .into_iter()
    .map(|(surface, bypassed)| (surface.to_owned(), bypassed))
    .collect();
    assert_eq!(marked, expected);
}
