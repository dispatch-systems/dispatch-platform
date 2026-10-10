//! The MCP's test support, for its own tests and, behind the `testing` feature, the app's: the
//! MCP with two stand-in tools in core's test registry, a key as an agent signs in with it,
//! and a tool called as the server calls it.
use super::{
    Caller, KeyStore,
    api::types::{AgentAccess, AgentKeyRequest},
    piece::{Own, piece},
    tools::{Answer, Cx, Effect, Nothing, Tool, Toolbox},
};
use dispatch_core::{
    db::Store,
    manifest::Agents,
    testing::{self, platform_owner},
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU32, Ordering};

/// The MCP with the stand-in tools, as its tests install it.
pub const AGENTS: Agents = piece(&Own {
    tools: &[&StandInRead, &StandInChange],
});

/// Installs core's test registry with the MCP, `AGENTS`, as its agents' piece.
pub fn install() {
    testing::install_with(Some(&AGENTS), &[], &[]);
}

/// A key the platform owner made, reaching `dsps` (every DSP when none) and allowed `tools`
/// with those added later that only read, as an agent signs in with it.
pub fn agent(db: &Store, dsps: &[&str], tools: &[&str]) -> Caller {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let made = MADE.fetch_add(1, Ordering::Relaxed);
    let request = AgentKeyRequest::parse(&json!({
        "name": format!("Test agent {made}"),
        "allDsps": dsps.is_empty(),
        "dsps": dsps,
        "access": AgentAccess::Read,
        "allTools": true,
        "tools": tools,
        "expiresAt": null,
    }))
    .unwrap();
    let key = db.create_agent_key(&platform_owner(db), &request).unwrap();
    db.authenticate_agent(&key.token, "test").unwrap()
}

/// Calls a tool of the installed registry as `caller`, checked as the MCP server checks a
/// call: `call_tool(&db, &caller, "approve_timecard", json!({"dsp": dsp, "date": "…"}))`.
pub fn call_tool(db: &Store, caller: &Caller, name: &str, arguments: Value) -> Answer<Value> {
    let Value::Object(arguments) = arguments else {
        panic!("a tool's arguments are an object");
    };
    Toolbox::installed()
        .invoke(db, caller, name, arguments)
        .answer
}

/// What a stand-in tool answers: the DSP it was handed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct StandInAnswer {
    pub dsp: String,
}
/// A tool of the stand-in's that reads, for tests of what a key may use.
pub struct StandInRead;
impl Tool for StandInRead {
    const NAME: &'static str = "stand_in_read";
    const TITLE: &'static str = "Stand-in read";
    const DESCRIPTION: &'static str = "Answer with the DSP.";
    type Input = Nothing;
    type Output = StandInAnswer;
    fn call(cx: &Cx, _: Nothing) -> Answer<StandInAnswer> {
        Ok(StandInAnswer {
            dsp: cx.dsp().id.clone(),
        })
    }
}
/// A tool of the stand-in's that changes something: it records that it did.
pub struct StandInChange;
impl Tool for StandInChange {
    const NAME: &'static str = "stand_in_change";
    const TITLE: &'static str = "Stand-in change";
    const DESCRIPTION: &'static str = "Record a change at the DSP.";
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = StandInAnswer;
    fn call(cx: &Cx, _: Nothing) -> Answer<StandInAnswer> {
        cx.audit("stand_in.changed", "Changed", None, &[])?;
        Ok(StandInAnswer {
            dsp: cx.dsp().id.clone(),
        })
    }
}
