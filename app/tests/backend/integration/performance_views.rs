//! Overlap and count units through the same handler used by REST and MCP.
use super::daily_performance::{caller, publish, publish_with, ready};
use dispatch_core::{
    State,
    db::{Store, s},
    mcp::data::{self, catalog, settle},
    testing,
};
use dispatch_cortex::{self as cortex, daily_performance, discovery::CollectionRequest};
use dispatch_daily_performance::DailyPerformanceStore;
use dispatch_weekly_scorecard::WeeklyScorecardStore;
use serde_json::{Value, json};
fn ask(store: &Store, caller: &dispatch_core::mcp::Caller, id: &str, query: Value) -> (u16, Value) {
    let state = State::new(store.config.clone()).unwrap();
    settle(data::ask(
        catalog::endpoint(id),
        store,
        &state,
        caller,
        "",
        &query,
    ))
    .unwrap()
}
fn returns(capture: &mut daily_performance::Capture, rows: Vec<Value>) {
    capture
        .datasets
        .iter_mut()
        .find(|d| d.id == daily_performance::dataset("returns_to_station").unwrap().id)
        .unwrap()
        .rows = rows;
}
fn attempt(tracking: &str, driver: &str, date: &str, impact: bool) -> Value {
    json!({"tracking_id":tracking,"transporter_id":driver,"delivery_planned_date":date,
        "rts_reason_code":"BUSINESS CLOSED","impacting_dcr":impact,
        "daily_coaching":if impact {"Contact customer"} else {""}})
}
fn weekly(store: &Store, dsp: &str) {
    weekly_with(store, dsp, |_| {});
}
fn weekly_with(
    store: &Store,
    dsp: &str,
    adjust: impl FnOnce(&mut cortex::weekly_scorecard::Capture),
) {
    let request = cortex::weekly_scorecard::Request::parse(
        &json!({"collection":"weekly_scorecard","week":"2026-W39","station":"TST1",
        "timezone":"America/Los_Angeles","dspName":"Fixture Delivery","dspAbbreviation":"FXTR"}),
    )
    .unwrap()
    .unwrap();
    let mut capture = cortex::weekly_scorecard::fixture(&request).unwrap();
    adjust(&mut capture);
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!("discover")
    };
    let job = store
        .enqueue_weekly_scorecard(dsp, None, "weekly-view", Some("2026-W39"))
        .unwrap();
    store
        .publish_weekly_scorecard(
            dsp,
            s(&job, "id"),
            &capture,
            &discovery.scope("area-fixture", "company-fixture").unwrap(),
        )
        .unwrap();
}
#[test]
fn latest_snapshots_replace_annotations_but_keep_distinct_attempts_and_missing_ids() {
    let (_root, store, dsp) = ready();
    publish_with(&store, &dsp, "2026-09-20", "first", |c| {
        returns(
            c,
            vec![
                attempt("A", "driver-1", "2026-09-20", true),
                attempt("B", "driver-2", "2026-09-20", true),
            ],
        )
    });
    publish_with(&store, &dsp, "2026-09-21", "later", |c| {
        returns(
            c,
            vec![
                attempt("A", "driver-1", "2026-09-20", false),
                attempt("A", "driver-2", "2026-09-20", true),
                attempt("A", "driver-1", "2026-09-21", true),
                attempt("B", "driver-2", "2026-09-20", true),
                attempt("", "driver-1", "2026-09-21", true),
                attempt("", "driver-1", "2026-09-21", true),
            ],
        )
    });
    let caller = caller(&store, &dsp, &["daily_returns"]);
    let query = json!({"view":"operational","from":"2026-09-20","to":"2026-09-21","detail":"full","limit":"2","group_by":"day"});
    let (status, answer) = ask(&store, &caller, "returns", query.clone());
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["contract_version"], 2);
    assert_eq!(answer["counts"]["records"], 6);
    assert_eq!(answer["counts"]["hurting_dcr"], 5);
    assert_eq!(answer["counts"]["contact_missed"], 5);
    assert_eq!(answer["counts"]["unidentified"], 2);
    assert_eq!(answer["list"]["page"]["total"], 6);
    let mut pages = answer["list"]["rows"].as_array().unwrap().clone();
    let mut page = answer;
    while let Some(next) = page["list"]["page"]["next_cursor"].as_str() {
        let mut query = query.clone();
        query["cursor"] = json!(next);
        let (status, answer) = ask(&store, &caller, "returns", query);
        assert_eq!(status, 200, "{answer}");
        pages.extend(answer["list"]["rows"].as_array().unwrap().iter().cloned());
        page = answer;
    }
    assert_eq!(pages.len(), 6);
    let (status, answer) = ask(
        &store,
        &caller,
        "returns",
        json!({"view":"operational","date":"2026-09-20","impacting":"true","contact":"missed"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["counts"]["records"], 2);
    let (_, answer) = ask(
        &store,
        &caller,
        "returns",
        json!({"view":"operational","date":"2026-09-20","driver":"driver-1"}),
    );
    assert_eq!(answer["counts"]["records"], 1);
    assert_eq!(answer["counts"]["hurting_dcr"], 0);
    let (_, legacy) = ask(
        &store,
        &caller,
        "returns",
        json!({"source":"daily_performance","from":"2026-09-20","to":"2026-09-21"}),
    );
    assert_eq!(legacy["recorded_rows"], 8);
    assert!(legacy.get("contract_version").is_none());
    assert_eq!(
        store
            .daily_performance_db(&dsp)
            .unwrap()
            .count(
                "SELECT count(*) FROM daily_rows WHERE dataset='returns_to_station'",
                []
            )
            .unwrap(),
        8
    );
}
#[test]
fn detail_and_official_aggregate_mismatches_are_explicit() {
    let (_root, store, dsp) = ready();
    publish_with(&store, &dsp, "2026-09-20", "aggregate", |c| {
        returns(c, vec![attempt("A", "driver-1", "2026-09-20", true)]);
        c.datasets
            .iter_mut()
            .find(|d| d.id == daily_performance::dataset("driver_returns").unwrap().id)
            .unwrap()
            .rows = vec![json!({"transporter_id":"driver-1","rts_all":2})];
    });
    let caller = caller(&store, &dsp, &["daily_returns"]);
    let (status, answer) = ask(
        &store,
        &caller,
        "returns",
        json!({"view":"operational","date":"2026-09-20","driver":"driver-1"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["counts"]["records"], 1);
    assert_eq!(answer["counts"]["reported_returns"], 2);
    assert_eq!(answer["reconciliation"]["status"], "mismatch");
}
#[test]
fn assessed_safety_replaces_live_copies_and_pending_events_stay_distinct() {
    let (_root, store, dsp) = ready();
    publish_with(&store, &dsp, "2026-09-20", "safety", |c| {
        for d in &mut c.datasets {
            if d.id == daily_performance::dataset("safety_events").unwrap().id {
                d.rows[0]["final_resolution"] = json!("Dispute Approved");
                d.rows[0]["severity"] = json!("assessed");
            }
            if d.id == daily_performance::dataset("live_safety_events").unwrap().id {
                let mut pending = d.rows[0].clone();
                pending["event_id"] = json!("pending-only");
                d.rows.push(pending);
            }
        }
    });
    let caller = caller(&store, &dsp, &["daily_safety"]);
    let (status, answer) = ask(
        &store,
        &caller,
        "safety",
        json!({"view":"operational","date":"2026-09-20","detail":"full"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["counts"]["records"], 2);
    assert_eq!(answer["counts"]["assessed"], 1);
    assert_eq!(answer["counts"]["live_only"], 1);
    assert_eq!(answer["list"]["rows"].as_array().unwrap().len(), 2);
    let (_, approved) = ask(
        &store,
        &caller,
        "safety",
        json!({"view":"operational","date":"2026-09-20","counting":"false"}),
    );
    assert_eq!(approved["counts"]["records"], 1);
    assert_eq!(approved["counts"]["live_only"], 0);
    let (_, legacy) = ask(
        &store,
        &caller,
        "safety",
        json!({"source":"daily_performance","date":"2026-09-20"}),
    );
    assert_eq!(legacy["recorded_rows"], 1);
}
#[test]
fn compare_gates_both_sources_and_keeps_units_and_pages_independent() {
    let (_root, store, dsp) = ready();
    weekly(&store, &dsp);
    publish_with(&store, &dsp, "2026-09-26", "compare", |c| {
        returns(
            c,
            vec![
                attempt("A", "driver-1", "2026-09-26", true),
                attempt("B", "driver-2", "2026-09-26", true),
            ],
        )
    });
    let daily_only = caller(&store, &dsp, &["daily_returns"]);
    let query =
        json!({"view":"compare","period":"2026-W39","detail":"full","limit":"1","group_by":"day"});
    assert_eq!(ask(&store, &daily_only, "returns", query.clone()).0, 403);
    let both = caller(
        &store,
        &dsp,
        &["daily_returns", "returns", "daily_feedback", "feedback"],
    );
    let (status, answer) = ask(&store, &both, "returns", query.clone());
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["sources"]["operational"]["counts"]["records"], 2);
    assert_eq!(answer["sources"]["operational"]["coverage"]["unit"], "days");
    assert_eq!(
        answer["sources"]["posted_scorecard"]["coverage"]["unit"],
        "weeks"
    );
    assert_eq!(
        answer["sources"]["posted_scorecard"]["coverage"]["status"],
        "complete"
    );
    assert_eq!(
        answer["sources"]["operational"]["list"]["page"]["cursor_parameter"],
        "operational_cursor"
    );
    assert_eq!(
        answer["sources"]["posted_scorecard"]["counts"]["records"],
        2
    );
    let original_weekly = answer["sources"]["posted_scorecard"]["list"].clone();
    let mut next = query.clone();
    next["operational_cursor"] =
        answer["sources"]["operational"]["list"]["page"]["next_cursor"].clone();
    let (status, page) = ask(&store, &both, "returns", next);
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["sources"]["posted_scorecard"]["list"], original_weekly);
    assert_ne!(
        page["sources"]["operational"]["list"]["rows"],
        answer["sources"]["operational"]["list"]["rows"]
    );
    let (_, feedback) = ask(
        &store,
        &both,
        "feedback",
        json!({"view":"compare","period":"2026-W39"}),
    );
    assert_eq!(
        feedback["sources"]["operational"]["counts"]["response_unit"],
        "feedback_responses"
    );
    assert_eq!(
        feedback["sources"]["posted_scorecard"]["counts"]["unit"],
        "package_feedback_records"
    );
    let (status, nonimpacting) = ask(
        &store,
        &both,
        "returns",
        json!({"view":"posted_scorecard","period":"2026-W39","impacting":"false"}),
    );
    assert_eq!(status, 200, "{nonimpacting}");
    assert_eq!(nonimpacting["counts"]["records"], 1);
    assert_eq!(
        ask(
            &store,
            &both,
            "returns",
            json!({"view":"operational","source":"weekly_scorecard"})
        )
        .0,
        400
    );
    assert_eq!(
        ask(
            &store,
            &both,
            "returns",
            json!({"view":"compare","cursor":"1"})
        )
        .0,
        400
    );
    store
        .platform
        .exec(
            "UPDATE dsp_features SET enabled=0 WHERE dsp_id=? AND feature='daily_performance'",
            [&dsp],
        )
        .unwrap();
    let (status, answer) = ask(&store, &both, "returns", query);
    assert_eq!(status, 403);
    assert_eq!(answer["error"], "source_off");
}
#[test]
fn large_comparisons_fit_the_budget_and_preserve_independent_group_and_detail_totals() {
    let (_root, store, dsp) = ready();
    let rows: Vec<Value> = (0..400)
        .map(|i| {
            let mut row = attempt(
                &format!("tracking-{i}"),
                &format!("transporter-{i}"),
                "2026-09-26",
                true,
            );
            row["weekly_coaching"] = json!("Contact ".repeat(100));
            row["daily_coaching"] = row["weekly_coaching"].clone();
            row
        })
        .collect();
    weekly_with(&store, &dsp, |c| {
        c.datasets
            .iter_mut()
            .find(|d| d.id == "da_dsp_weekly_rts_deep_dive")
            .unwrap()
            .rows = rows.clone()
    });
    publish_with(&store, &dsp, "2026-09-26", "bulk", |c| returns(c, rows));
    let caller = caller(&store, &dsp, &["daily_returns", "returns"]);
    let (status, answer) = ask(
        &store,
        &caller,
        "returns",
        json!({"view":"compare","period":"2026-W39","detail":"full","group_by":"driver","limit":"500"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert!(answer.to_string().len() <= data::BUDGET);
    for view in ["operational", "posted_scorecard"] {
        assert_eq!(answer["sources"][view]["counts"]["records"], 400);
        for table in ["list", "groups"] {
            assert_eq!(answer["sources"][view][table]["page"]["total"], 400);
            assert!(
                answer["sources"][view][table]["rows"]
                    .as_array()
                    .unwrap()
                    .len()
                    > 0
            );
            assert!(answer["sources"][view][table]["page"]["next_cursor"].is_string());
        }
    }
}
#[test]
fn the_default_daily_refresh_can_queue_all_seven_dates() {
    let (_root, store, dsp) = ready();
    let requests: Vec<_> = store
        .daily_performance_jobs(&dsp)
        .unwrap()
        .into_iter()
        .map(|(key, request)| (key, cortex::PROVIDER, request))
        .collect();
    assert_eq!(requests.len(), 7);
    assert_eq!(store.enqueue_batch(&dsp, None, &requests).unwrap().len(), 7);
}
#[test]
fn weekly_dsp_feedback_counters_stay_distinct_from_package_reviews() {
    let (_root, store, dsp) = ready();
    weekly_with(&store, &dsp, |c| {
        let reviews = c
            .datasets
            .iter_mut()
            .find(|d| d.id == "da_dsp_weekly_cdf_deep_dive")
            .unwrap();
        for row in &mut reviews.rows {
            row["delivery_time"] = json!("2026-09-26");
            row["negative_feedback_flag"] = json!(1);
        }
        c.datasets
            .iter_mut()
            .find(|d| d.id == "dsp_weekly_cdf")
            .unwrap()
            .rows[0]["negative_response_cnt"] = json!(7);
    });
    publish(&store, &dsp, "2026-09-26", "feedback");
    let caller = caller(&store, &dsp, &["daily_feedback", "feedback"]);
    let (status, answer) = ask(
        &store,
        &caller,
        "feedback",
        json!({"view":"compare","period":"2026-W39"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["sources"]["operational"]["counts"]["responses"], 3);
    assert_eq!(
        answer["sources"]["posted_scorecard"]["counts"]["records"],
        2
    );
    assert_eq!(
        answer["sources"]["posted_scorecard"]["counts"]["reported_feedback"]["negative"],
        7
    );
    let (_, day) = ask(
        &store,
        &caller,
        "feedback",
        json!({"view":"posted_scorecard","date":"2026-09-26"}),
    );
    assert!(day["counts"].get("reported_feedback").is_none());
}
#[test]
fn absent_data_is_unknown_and_an_unavailable_comparison_never_becomes_zero() {
    let (_root, store, dsp) = ready();
    weekly(&store, &dsp);
    let both = caller(&store, &dsp, &["daily_returns", "returns"]);
    let (status, answer) = ask(
        &store,
        &both,
        "returns",
        json!({"view":"compare","period":"2026-W39"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert!(answer["sources"]["operational"]["counts"].is_null());
    assert_eq!(
        answer["sources"]["operational"]["coverage"]["status"],
        "unavailable"
    );
    assert_eq!(
        answer["sources"]["posted_scorecard"]["counts"]["records"],
        2
    );
    assert_eq!(
        ask(
            &store,
            &both,
            "returns",
            json!({"view":"operational","period":"2026-W39"})
        )
        .0,
        404
    );
}
#[test]
fn daily_backfills_admit_31_dates_atomically_with_idempotency_and_existing_bounds() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(
            &dsp,
            Some(&testing::platform_owner(&store)),
            "month",
            "2026-09-01",
            "2026-10-01",
        )
        .unwrap();
    assert_eq!(queued.as_array().unwrap().len(), 31);
    assert_eq!(
        store
            .enqueue_daily_performance(
                &dsp,
                Some(&testing::platform_owner(&store)),
                "month",
                "2026-09-01",
                "2026-10-01"
            )
            .unwrap(),
        queued
    );
    assert_eq!(
        store
            .enqueue_daily_performance(&dsp, None, "overflow", "2026-10-02", "2026-10-02")
            .unwrap_err()
            .code,
        "queue_full"
    );
    assert_eq!(
        store
            .recent_jobs_of(&dsp, daily_performance::JOB_KIND)
            .unwrap()
            .len(),
        31
    );
    assert_eq!(
        store
            .enqueue_daily_performance(&dsp, None, "too-long", "2026-09-01", "2026-10-02")
            .unwrap_err()
            .code,
        "invalid_date_range"
    );
}
