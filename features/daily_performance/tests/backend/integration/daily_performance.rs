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
use dispatch_daily_performance::{DailyPerformancePolicy, DailyPerformanceStore};
use serde_json::{Value, json};
fn ready() -> (tempfile::TempDir, Store, String) {
    testing::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_daily_performance::FEATURE],
    );
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
fn publish(store: &Store, dsp: &str, date: &str, key: &str) -> daily_performance::Capture {
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
    let capture = daily_performance::fixture(&request).unwrap();
    store
        .publish_daily_performance(dsp, job, &capture, &scope)
        .unwrap();
    capture
}
fn caller(store: &Store, dsp: &str, areas: &[&str]) -> dispatch_core::mcp::Caller {
    let input = AgentKeyRequest::parse(
        &json!({"name":"daily test","allDsps":false,"dsps":[dsp],"access":"read",
        "reads":{"areas":areas,"bypass":false},"dspReads":[],"expiresAt":null}),
    )
    .unwrap();
    let key = store
        .create_agent_key(&testing::platform_owner(store), &input)
        .unwrap();
    store.authenticate_agent(&key.token, "test").unwrap()
}
#[test]
fn daily_publications_supersede_without_losing_history_and_empty_datasets_are_unconfirmed() {
    let (_root, store, dsp) = ready();
    let first = publish(&store, &dsp, "2026-10-03", "first");
    let job = store
        .enqueue_daily_performance(&dsp, None, "first", "2026-10-03", "2026-10-03")
        .unwrap();
    let request = Request {
        collection: daily_performance::Collection::DailyPerformance,
        date: "2026-10-03".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Delivery".into(),
        dsp_abbreviation: "FXTR".into(),
    };
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!()
    };
    store
        .publish_daily_performance(
            &dsp,
            s(&job[0], "id"),
            &first,
            &discovery.scope("area-fixture", "company-fixture").unwrap(),
        )
        .unwrap();
    assert_eq!(
        store
            .daily_performance_db(&dsp)
            .unwrap()
            .count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        1
    );
    publish(&store, &dsp, "2026-10-03", "again");
    let data = store.daily_performance_db(&dsp).unwrap();
    assert_eq!(
        data.count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        2
    );
    assert_eq!(
        data.count("SELECT count(*) FROM daily_publications WHERE active=1", [])
            .unwrap(),
        1
    );
    assert_eq!(
        data.count("SELECT count(*) FROM daily_rows", []).unwrap(),
        2 * first.row_count() as i64
    );
    let days = store.daily_performance_days(&dsp).unwrap();
    assert_eq!(days.days.len(), 1);
    let contacts = days.days[0]
        .datasets
        .iter()
        .find(|dataset| dataset.dataset == "driver_contacts")
        .unwrap();
    assert_eq!(
        (contacts.rows, contacts.coverage.as_str()),
        (0, "unconfirmed")
    );
}
#[test]
fn cross_date_and_cross_company_rows_are_rejected_before_any_publication_changes() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(&dsp, None, "scope", "2026-10-03", "2026-10-03")
        .unwrap();
    let job = s(&queued[0], "id");
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let payload: Value =
        serde_json::from_str(&store.job_row(job, Some(&dsp)).unwrap().request).unwrap();
    let request: Request =
        serde_json::from_value(keeper.bind(&store, &dsp, &payload).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!()
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let capture = daily_performance::fixture(&request).unwrap();
    for (field, value) in [
        ("data_date", "2026-10-02"),
        ("company_id", "foreign"),
        ("station_code", "OTHER"),
    ] {
        let mut wrong = capture.clone();
        wrong.datasets[0].rows[0][field] = json!(value);
        assert!(
            store
                .publish_daily_performance(&dsp, job, &wrong, &scope)
                .is_err()
        );
    }
    assert_eq!(
        store
            .daily_performance_db(&dsp)
            .unwrap()
            .count("SELECT count(*) FROM daily_publications", [])
            .unwrap(),
        0
    );
}
#[test]
fn one_driver_filters_before_counting_and_every_driver_can_be_paged_over_a_range() {
    let (_root, store, dsp) = ready();
    publish(&store, &dsp, "2026-10-03", "oct3");
    publish(&store, &dsp, "2026-10-04", "oct4");
    let caller = caller(&store, &dsp, &["daily_performance"]);
    let state = State::new(store.config.clone()).unwrap();
    let ask = |query| {
        settle(data::ask(
            catalog::endpoint("daily_performance"),
            &store,
            &state,
            &caller,
            "",
            &query,
        ))
        .unwrap()
    };
    let (status, answer) = ask(
        json!({"from":"2026-10-03","to":"2026-10-04","driver":"driver-2",
        "detail":"full","fields":"delivered,pod_success_rate"}),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["recorded_rows"], 2);
    assert_eq!(answer["list"]["rows"].as_array().unwrap().len(), 2);
    assert!(
        answer["list"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row[1].as_str().unwrap().contains("Driver 2"))
    );
    let (_, answer) =
        ask(json!({"from":"2026-10-03","to":"2026-10-04","detail":"full","limit":"2"}));
    assert_eq!(answer["recorded_rows"], 6);
    assert_eq!(answer["list"]["page"]["next_cursor"], "2");
    let (_, next) = ask(
        json!({"from":"2026-10-03","to":"2026-10-04","detail":"full","limit":"2","cursor":"2"}),
    );
    assert_eq!(next["list"]["page"]["next_cursor"], "4");
    assert_ne!(answer["list"]["rows"], next["list"]["rows"]);
    let (status, missing) = ask(json!({"date":"2026-10-03","dataset":"driver_contacts"}));
    assert_eq!(status, 404, "{missing}");
    assert_eq!(missing["error"], "data_unavailable");
}
#[test]
fn schedules_refresh_configured_dates_and_manual_backfills_are_bounded_and_idempotent() {
    let (_root, store, dsp) = ready();
    store
        .set_daily_performance_policy(
            &dsp,
            &DailyPerformancePolicy {
                lookback_days: 3,
                refresh_hours: 20,
            },
        )
        .unwrap();
    let jobs = store.daily_performance_jobs(&dsp).unwrap();
    assert_eq!(jobs.len(), 3);
    let date = s(&jobs[0].1, "date").to_owned();
    publish(&store, &dsp, &date, "recent");
    assert_eq!(store.daily_performance_jobs(&dsp).unwrap().len(), 2);
    store
        .daily_performance_db(&dsp)
        .unwrap()
        .exec(
            "UPDATE daily_publications SET collected_at='2020-01-01'",
            [],
        )
        .unwrap();
    assert_eq!(store.daily_performance_jobs(&dsp).unwrap().len(), 3);
    let first = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 3);
    let again = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first, again);
    assert!(
        store
            .enqueue_daily_performance(&dsp, None, "huge", "2026-01-01", "2026-10-03")
            .is_err()
    );
    assert!(
        store
            .set_daily_performance_policy(
                &dsp,
                &DailyPerformancePolicy {
                    lookback_days: 0,
                    refresh_hours: 20
                }
            )
            .is_err()
    );
}

#[test]
fn feedback_counts_and_categories_are_aggregated_without_inventing_individual_reviews() {
    let (_root, store, dsp) = ready();
    publish(&store, &dsp, "2026-10-03", "feedback");
    let caller = caller(&store, &dsp, &["daily_performance"]);
    let state = State::new(store.config.clone()).unwrap();
    let ask = |query| {
        settle(data::ask(
            catalog::endpoint("daily_performance"),
            &store,
            &state,
            &caller,
            "",
            &query,
        ))
        .unwrap()
    };
    let (status, answer) = ask(json!({"date":"2026-10-03","dataset":"driver_feedback",
        "type":"mishandled","feedback":"negative","group_by":"driver","list":"true"}));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["recorded_rows"], 2);
    assert_eq!(answer["feedback_counts"]["negative"], 3);
    assert_eq!(answer["feedback_counts"]["selected_type"]["count"], 3);
    assert_eq!(
        answer["groups"]["columns"],
        json!([
            "driver",
            "recorded_rows",
            "positive_responses",
            "negative_responses"
        ])
    );
    assert_eq!(answer["groups"]["rows"].as_array().unwrap().len(), 2);
    for query in [
        json!({"dataset":"dsp_quality","driver":"driver-2"}),
        json!({"dataset":"driver_quality","group_by":"reason"}),
        json!({"dataset":"driver_quality","type":"speeding"}),
        json!({"dataset":"driver_quality","contact":"all"}),
        json!({"dataset":"driver_feedback","type":"fabricated"}),
        json!({"dataset":"driver_quality","fields":"latitude,video_url"}),
    ] {
        let mut query = query;
        query["date"] = json!("2026-10-03");
        let (status, answer) = ask(query);
        assert_eq!(status, 400, "{answer}");
    }
}
#[test]
fn assessed_safety_dispute_filters_normalize_provider_words_and_group_by_actual_type() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(&dsp, None, "disputes", "2026-10-03", "2026-10-03")
        .unwrap();
    let job = s(&queued[0], "id");
    let keeper = dispatch_core::manifest::registry().keeper(daily_performance::JOB_KIND);
    let payload: Value =
        serde_json::from_str(&store.job_row(job, Some(&dsp)).unwrap().request).unwrap();
    let request: Request =
        serde_json::from_value(keeper.bind(&store, &dsp, &payload).unwrap()).unwrap();
    let CollectionRequest::Discover(discovery) = request.scope_request() else {
        panic!()
    };
    let scope = discovery.scope("area-fixture", "company-fixture").unwrap();
    let mut capture = daily_performance::fixture(&request).unwrap();
    let events = capture
        .datasets
        .iter_mut()
        .find(|dataset| dataset.id.ends_with("events_intraday"))
        .unwrap();
    let template = events.rows[0].clone();
    events.rows = ["Dispute Approved", "Dispute Denied", ""]
        .iter()
        .enumerate()
        .map(|(index, resolution)| {
            let mut row = template.clone();
            row["final_resolution"] = json!(resolution);
            row["event_id"] = json!(format!("event-{index}"));
            row
        })
        .collect();
    store
        .publish_daily_performance(&dsp, job, &capture, &scope)
        .unwrap();
    let caller = caller(&store, &dsp, &["daily_performance"]);
    let state = State::new(store.config.clone()).unwrap();
    for counting in ["true", "false"] {
        let (status, answer) = settle(data::ask(
            catalog::endpoint("daily_performance"),
            &store,
            &state,
            &caller,
            "",
            &json!({"date":"2026-10-03","dataset":"safety_events",
                "counting":counting,"type":"speeding","group_by":"type"}),
        ))
        .unwrap();
        assert_eq!(status, 200, "{answer}");
        assert_eq!(answer["recorded_rows"], 1);
        assert_eq!(answer["groups"]["rows"], json!([["speeding", 1]]));
    }
}
