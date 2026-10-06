//! Daily isolation, publication safety, driver filtering, bounded pages and scheduling.
use dispatch_core::{
    State,
    db::{Store, s},
    mcp::{
        api::types::AgentKeyRequest,
        data::{self, catalog, settle},
    },
    testing,
};
use dispatch_cortex::{
    self as cortex,
    daily_performance::{self, Request},
    discovery::CollectionRequest,
};
use dispatch_daily_performance::DailyPerformanceStore;
use serde_json::{Value, json};
pub(super) fn ready() -> (tempfile::TempDir, Store, String) {
    dispatch_backend::install();
    let (root, store, dsp) = testing::bootstrapped();
    testing::set_dsp(&store, &dsp, "Fixture Delivery", "America/Los_Angeles").unwrap();
    store
        .set_profile(
            &dsp,
            json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
        )
        .unwrap();
    testing::ready_connection(&store, &dsp, cortex::PROVIDER).unwrap();
    store.enable_all_features(&dsp).unwrap();
    (root, store, dsp)
}
pub(super) fn publish(
    store: &Store,
    dsp: &str,
    date: &str,
    key: &str,
) -> daily_performance::Capture {
    publish_with(store, dsp, date, key, |_| {})
}
pub(super) fn publish_with(
    store: &Store,
    dsp: &str,
    date: &str,
    key: &str,
    adjust: impl FnOnce(&mut daily_performance::Capture),
) -> daily_performance::Capture {
    let queued = store
        .enqueue_daily_performance(dsp, None, key, date, date)
        .unwrap();
    let job = s(&queued[0], "id");
    let request: Value =
        serde_json::from_str(&store.job_row(job, Some(dsp)).unwrap().request).unwrap();
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let request: Request =
        serde_json::from_value(keeper.bind(store, dsp, &request).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!("discover")
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let mut capture = daily_performance::fixture(&request).unwrap();
    adjust(&mut capture);
    store
        .publish_daily_performance(dsp, job, &capture, &scope)
        .unwrap();
    capture
}

pub(super) fn caller(store: &Store, dsp: &str, areas: &[&str]) -> dispatch_core::mcp::Caller {
    let input = AgentKeyRequest::parse(
        &json!({"name":format!("daily {}",areas.join("+")),"allDsps":false,"dsps":[dsp],"access":"read",
        "reads":{"areas":areas,"bypass":false},"dspReads":[],"expiresAt":null}),
    )
    .unwrap();
    let key = store
        .create_agent_key(&testing::platform_owner(store), &input)
        .unwrap();
    store.authenticate_agent(&key.token, "test").unwrap()
}
#[test]
fn focused_daily_tools_gate_their_own_reads_and_live_safety_never_gets_added() {
    let (_root, store, dsp) = ready();
    publish(&store, &dsp, "2026-10-04", "events");
    let caller = caller(&store, &dsp, &["daily_safety", "daily_returns"]);
    let state = State::new(store.config.clone()).unwrap();
    let ask = |id, query| {
        settle(data::ask(
            catalog::endpoint(id),
            &store,
            &state,
            &caller,
            "",
            &query,
        ))
        .unwrap()
    };
    let (status, answer) = ask(
        "safety",
        json!({"source":"daily_performance","date":"2026-10-04","list":"true"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["recorded_rows"], 1);
    assert_eq!(answer["dataset"], "safety_events");
    let (status, answer) = ask(
        "returns",
        json!({"source":"daily_performance","date":"2026-10-04",
        "reason":"business_closed","impacting":"true","contact":"missed","group_by":"driver","list":"true"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["recorded_rows"], 1);
    assert_eq!(answer["source"], "daily_performance");
    let (status, answer) = ask("daily_performance", json!({"date":"2026-10-04"}));
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["error"], "not_allowed");
}
