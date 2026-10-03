//! The agent API as agents meet it, held to what it was before the features declared their
//! parts of it: the OpenAPI document, the MCP server's handshake, tools and prompts, the Agent
//! Skill, the metrics and the kinds of data a key may read with the switches each follows.
//! Each is compared with its file in `app/tests/backend/agent_api/`;
//! `DISPATCH_UPDATE_AGENT_API=1` writes them anew.
#[path = "../../../../core/db/tests/support/common.rs"]
mod common;
use dispatch_backend::{
    State,
    agents::{Caller, data, skill},
    contracts::{AgentArea, AgentKeyRequest, AgentSource},
    db::{Store, s},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

const ORIGIN: &str = "https://dispatch.example.com";

fn golden(name: &str, text: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/backend/agent_api")
        .join(name);
    if std::env::var_os("DISPATCH_UPDATE_AGENT_API").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        stored == text,
        "{name} differs from what the agent API answers now"
    );
}
fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}

/// Every kind of data a key may read, in their order, with its label and the feature it
/// is read from.
fn areas() -> Value {
    json!(
        AgentArea::ALL
            .iter()
            .map(|area| json!({"id": area.as_str(), "label": area.label(),
                "source": area.source().as_str()}))
            .collect::<Vec<_>>()
    )
}
/// Every feature agents read from, in their order, with its switch's name.
fn sources() -> Value {
    json!(
        AgentSource::ALL
            .iter()
            .map(|source| json!({"id": source.as_str(), "switch": source.switch()}))
            .collect::<Vec<_>>()
    )
}
fn switched_on(db: &Store, dsp: &str) -> Value {
    json!(
        data::switched_on(db, dsp)
            .unwrap()
            .iter()
            .map(|source| source.as_str())
            .collect::<Vec<_>>()
    )
}

#[test]
fn the_openapi_document_skill_and_metrics_are_as_they_were() {
    dispatch_backend::install();
    golden("openapi.json", &pretty(&data::catalog::openapi(ORIGIN)));
    golden("skill.md", &skill::skill(ORIGIN));
    golden("metrics.json", &pretty(&data::catalog::metrics()));
}

#[test]
fn the_kinds_of_data_and_their_switches_are_as_they_were() {
    let (_root, db, dsp) = common::bootstrapped();
    db.enable_all_features(&dsp).unwrap();
    let mut switches = vec![json!({"off": [], "on": switched_on(&db, &dsp)})];
    let catalog: Vec<String> = db.features(&dsp).unwrap();
    // Each feature switched off, alone and beside each other, and what agents then read from.
    for (index, first) in catalog.iter().enumerate() {
        for second in std::iter::once(None).chain(catalog[index + 1..].iter().map(Some)) {
            let off: Vec<&String> = std::iter::once(first).chain(second).collect();
            for feature in &off {
                db.platform
                    .exec(
                        "UPDATE dsp_features SET enabled=0 WHERE dsp_id=? AND feature=?",
                        [dsp.as_str(), feature.as_str()],
                    )
                    .unwrap();
            }
            switches.push(json!({"off": off, "on": switched_on(&db, &dsp)}));
            db.enable_all_features(&dsp).unwrap();
        }
    }
    golden(
        "reads.json",
        &pretty(&json!({"areas": areas(), "sources": sources(), "switches": switches})),
    );
}

fn owner(db: &Store) -> String {
    s(
        &db.platform
            .one("SELECT id FROM users WHERE email='owner@example.test'", [])
            .unwrap()
            .unwrap(),
        "id",
    )
    .to_owned()
}
fn key(db: &Store, dsp: &str, name: &str, areas: &[&str]) -> Caller {
    let request = AgentKeyRequest::parse(&json!({"name": name, "allDsps": false,
        "dsps": [dsp], "access": "read", "reads": {"areas": areas, "bypass": false},
        "dspReads": [], "expiresAt": null}))
    .unwrap();
    let made = db.create_agent_key(&owner(db), &request).unwrap();
    db.authenticate_agent(&made.token, "test").unwrap()
}

/// One MCP message as a client speaking `protocol` sends it, and the server's JSON answer.
async fn mcp(
    state: &Arc<State>,
    caller: &Caller,
    method: &str,
    params: Value,
    protocol: &str,
) -> Value {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    let modern = protocol >= "2026-07-28";
    let mut params = params;
    if modern {
        params["_meta"] = json!({
            "io.modelcontextprotocol/protocolVersion": protocol,
            "io.modelcontextprotocol/clientInfo": {"name": "codex", "version": "0.157.1"},
            "io.modelcontextprotocol/clientCapabilities": {},
        });
    }
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/mcp")
        .header("host", "dispatch.test")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", protocol);
    if modern {
        request = request.header("mcp-method", method);
    }
    let mut request = request
        .body(Body::from(
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string(),
        ))
        .unwrap();
    request.extensions_mut().insert(state.clone());
    request.extensions_mut().insert(caller.clone());
    let response = dispatch_backend::agents::mcp::serve(request).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn the_mcp_server_offers_the_same_tools_and_prompts() {
    let (_root, db, dsp) = common::bootstrapped();
    let every = [
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
    let full = key(&db, &dsp, "Every kind", &every);
    let few = key(&db, &dsp, "Routes and DVIC", &["routes", "dvic"]);
    let config = db.config.clone();
    drop(db);
    let state = State::new(config).unwrap();
    let hello = json!({"protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "claude-code", "version": "2.1.283"}});
    let mut answers = serde_json::Map::new();
    answers.insert(
        "initialize".into(),
        mcp(&state, &full, "initialize", hello, "2025-06-18").await,
    );
    for protocol in ["2025-03-26", "2025-06-18", "2026-07-28"] {
        for method in ["tools/list", "prompts/list"] {
            answers.insert(
                format!("{method} {protocol}"),
                mcp(&state, &full, method, json!({}), protocol).await,
            );
        }
    }
    let offered = mcp(&state, &few, "tools/list", json!({}), "2025-06-18").await;
    let names: Vec<&Value> = offered["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| &tool["name"])
        .collect();
    answers.insert("tools/list routes and dvic".into(), json!(names));
    for (name, arguments) in [
        ("daily_summary", json!({})),
        (
            "daily_summary",
            json!({"date": "last night", "dsp": "Test DSP"}),
        ),
        ("driver_review", json!({"driver": "Avery Morgan"})),
        (
            "driver_review",
            json!({"driver": "Daniel", "period": "2026-W39"}),
        ),
    ] {
        let params = json!({"name": name, "arguments": arguments});
        answers.insert(
            format!("prompts/get {params}"),
            mcp(&state, &full, "prompts/get", params, "2025-06-18").await,
        );
    }
    golden("mcp.json", &pretty(&Value::Object(answers)));
}
