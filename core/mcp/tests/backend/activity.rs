use super::*;
use crate::mcp::api::types::AgentKeyKind;

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
    crate::testing::install(&[], &[]);
    assert_eq!(surface("/api/v1/whoami").unwrap(), "rest:whoami");
    assert_eq!(surface("/api/v1/mcp"), None);
    assert_eq!(surface("/api/platform/agents"), None);
    let message = |method: &str, params: serde_json::Value| {
        json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}).to_string()
    };
    let called = message(
        "tools/call",
        json!({"name":"whoami","arguments":{"driver":"Avery Morgan"}}),
    );
    assert_eq!(tool_call(called.as_bytes()).unwrap(), "mcp:whoami");
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
fn every_page_of_the_log_reads_one_index_in_order() {
    crate::testing::install(&[], &[]);
    let (_root, db) = crate::testing::store();
    let query = |key: Option<&str>, outcomes, before| ActivityQuery {
        key: key.map(Into::into),
        outcomes,
        before,
        limit: 50,
    };
    let plans: Vec<String> = [
        query(None, Outcomes::All, None),
        query(Some("k"), Outcomes::All, None),
        query(None, Outcomes::Ok, None),
        query(None, Outcomes::Refused, None),
        query(None, Outcomes::All, Some((1, 2))),
    ]
    .iter()
    .map(|query| {
        let (sql, values) = page(query);
        db.platform
            .all(
                &format!("EXPLAIN QUERY PLAN {sql}"),
                rusqlite::params_from_iter(values),
            )
            .unwrap()
            .iter()
            .map(|row| row["detail"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
            .join("; ")
    })
    .collect();
    for plan in &plans {
        assert!(plan.contains("USING INDEX agent_activity_"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
    assert!(plans[1].contains("agent_activity_key"), "{}", plans[1]);
    assert!(plans[3].contains("agent_activity_refused"), "{}", plans[3]);
}
