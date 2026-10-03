use super::*;
use crate::contracts::AgentKeyKind;

fn call(at: i64, key: &str, outcome: &str) -> Call {
    Call {
        at,
        key: AgentActivityKey {
            id: key.into(),
            name: "Laptop".into(),
            kind: AgentKeyKind::Key,
        },
        surface: "rest:whoami".into(),
        dsp: None,
        outcome: outcome.into(),
        bypassed: false,
        ms: 3,
        bytes: 120,
    }
}

#[test]
fn a_full_buffer_drops_the_oldest_and_counts_them() {
    let activity = Activity::default();
    // Two keys, so neither passes its day's cap.
    let key = |at: i64| if at % 2 == 0 { "key_a" } else { "key_b" };
    for at in 0..HELD as i64 + 5 {
        activity.record(call(at, key(at), "ok"));
    }
    assert_eq!(activity.pending(), HELD);
    let taken = activity.take(BATCH);
    assert_eq!(taken.dropped, 5);
    assert_eq!(taken.calls.len(), BATCH);
    assert_eq!(taken.calls[0].at, 5);
    // Writing them down failed: they wait again ahead of the rest, and nothing more goes.
    activity.restore(taken.calls);
    assert_eq!(activity.pending(), HELD);
    let again = activity.take(1);
    assert_eq!((again.calls[0].at, again.dropped), (5, 0));
    // Held again when full, the oldest of them go first.
    activity.record(call(HELD as i64 + 5, key(HELD as i64 + 5), "ok"));
    activity.restore(again.calls);
    let last = activity.take(HELD);
    assert_eq!((last.calls[0].at, last.dropped), (6, 1));
    assert_eq!(activity.pending(), 0);
}

#[test]
fn a_key_refused_for_its_rate_is_recorded_once_a_minute() {
    let activity = Activity::default();
    let minute = 1_790_000_040_000 / 60_000 * 60_000;
    for at in [minute, minute + 10, minute + 59_999] {
        activity.record(call(at, "key_a", "rate_limited"));
        activity.record(call(at, "key_b", "rate_limited"));
        activity.record(call(at, "key_a", "ok"));
    }
    activity.record(call(minute + 60_000, "key_a", "rate_limited"));
    let outcomes: Vec<(String, String)> = activity
        .take(HELD)
        .calls
        .into_iter()
        .map(|c| (c.key.id, c.outcome))
        .collect();
    let limited = |key: &str| {
        outcomes
            .iter()
            .filter(|(k, o)| k == key && o == "rate_limited")
            .count()
    };
    assert_eq!((limited("key_a"), limited("key_b")), (2, 1));
    assert_eq!(outcomes.iter().filter(|(_, o)| o == "ok").count(), 3);
}

#[test]
fn a_key_keeps_its_days_first_calls_then_one_row_that_marks_the_day_capped() {
    let activity = Activity::default();
    let day = 1_790_000_000_000 / DAY_MS * DAY_MS;
    for n in 0..i64::from(DAILY) {
        activity.record(call(day + n, "key_a", "ok"));
    }
    let first = activity.take(HELD);
    assert_eq!((first.calls.len(), first.capped), (DAILY as usize, 0));
    // A refusal repeated within its minute is not one more call.
    for n in 0..5 {
        activity.record(call(day + 50_000 + n, "key_a", "rate_limited"));
        activity.record(call(day + 50_000 + n, "key_a", "ok"));
    }
    activity.record(call(day + 60_000, "key_b", "ok"));
    activity.record(call(day + DAY_MS, "key_a", "ok"));
    let taken = activity.take(HELD);
    assert_eq!(taken.capped, 6);
    let seen: Vec<(&str, &str, &str, i64)> = taken
        .calls
        .iter()
        .map(|c| {
            (
                c.key.id.as_str(),
                c.surface.as_str(),
                c.outcome.as_str(),
                c.at,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            ("key_a", CAPPED_SURFACE, CAPPED, day + 50_000),
            ("key_b", "rest:whoami", "ok", day + 60_000),
            ("key_a", "rest:whoami", "ok", day + DAY_MS),
        ]
    );
    assert_eq!((taken.calls[0].ms, taken.calls[0].bytes), (0, 0));
    // A call that started the day before is still today's, and does not reopen it.
    activity.record(call(day + DAY_MS - 1, "key_a", "ok"));
    assert_eq!(activity.take(HELD).calls[0].at, day + DAY_MS - 1);
}

#[test]
fn calls_are_named_by_endpoint_or_tool_and_never_by_what_they_ask() {
    assert_eq!(surface("/api/v1/whoami").unwrap(), "rest:whoami");
    assert_eq!(surface("/api/v1/drivers/{driver}").unwrap(), "rest:driver");
    assert_eq!(surface("/api/v1/skill").unwrap(), "rest:skill");
    assert_eq!(surface("/api/v1/mcp"), None);
    let message = |method: &str, params: serde_json::Value| {
        json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string()
    };
    let called = message(
        "tools/call",
        json!({"name":"driver_report","arguments":{"driver":"Avery Morgan"}}),
    );
    assert_eq!(tool_call(called.as_bytes()).unwrap(), "mcp:driver_report");
    let invented = message("tools/call", json!({"name":"x".repeat(25_000)}));
    assert_eq!(tool_call(invented.as_bytes()).unwrap(), "mcp:unknown");
    assert_eq!(tool_call(message("tools/list", json!({})).as_bytes()), None);
    assert_eq!(tool_call(message("initialize", json!({})).as_bytes()), None);
    assert_eq!(tool_call(b"not json"), None);
    assert_eq!(cursor("1790000000000.42"), Some((1_790_000_000_000, 42)));
    assert_eq!(cursor("42"), None);
    assert_eq!(cursor("a.b"), None);
}

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
