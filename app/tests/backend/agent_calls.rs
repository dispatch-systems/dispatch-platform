//! What an agent's call records, through a registered feature's endpoint too.
use dispatch_core::mcp::activity::*;
use dispatch_core::mcp::api::types::AgentActivityKey;
use dispatch_core::mcp::api::types::AgentKeyKind;
#[test]
fn what_a_request_noted_becomes_its_call() {
    let activity = Activity::default();
    let key = AgentActivityKey {
        id: "agentkey_a".into(),
        name: "Laptop".into(),
        kind: AgentKeyKind::App,
    };
    let answered = |path, status, failed| Answered {
        path,
        at: 1,
        ms: 12,
        bytes: 300,
        status,
        failed,
    };
    let noted = |surface: Option<&str>, outcome: Option<&str>| Noted {
        key: Some(key.clone()),
        surface: surface.map(str::to_owned),
        dsp: None,
        outcome: outcome.map(str::to_owned),
        bypassed: false,
    };
    // No key signed it, or an MCP message that calls no tool: nothing to record.
    activity.finish(Noted::default(), answered("/api/v1/whoami", 401, Some("x")));
    activity.finish(noted(None, None), answered("/api/v1/mcp", 200, None));
    assert_eq!(activity.pending(), 0);
    activity.finish(noted(None, None), answered("/api/v1/skill", 200, None));
    activity.finish(
        noted(None, Some("dsp_required")),
        answered("/api/v1/team", 400, None),
    );
    activity.finish(
        noted(Some("mcp:whoami"), None),
        answered("/api/v1/mcp", 429, Some("rate_limited")),
    );
    activity.finish(
        noted(Some("mcp:whoami"), None),
        answered("/api/v1/mcp", 200, None),
    );
    // An answer read by bypassing features is marked so.
    activity.finish(
        Noted {
            bypassed: true,
            ..noted(None, Some("ok"))
        },
        answered("/api/v1/dvic", 200, None),
    );
    let calls = activity.take(HELD).calls;
    let seen: Vec<(&str, &str, bool)> = calls
        .iter()
        .map(|c| (c.surface.as_str(), c.outcome.as_str(), c.bypassed))
        .collect();
    assert_eq!(
        seen,
        [
            ("rest:skill", "ok", false),
            ("rest:team", "dsp_required", false),
            ("mcp:whoami", "rate_limited", false),
            ("mcp:whoami", "invalid_request", false),
            ("rest:dvic", "ok", true),
        ]
    );
    assert_eq!((calls[0].ms, calls[0].bytes), (12, 300));
    assert_eq!(calls[0].key.kind, AgentKeyKind::App);
}
