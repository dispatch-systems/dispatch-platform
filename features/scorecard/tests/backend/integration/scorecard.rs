//! Scorecard storage: publication, supersession, weeks not posted, where the next
//! job reads and what a schedule queues.
use dispatch_core::State;
use dispatch_core::db::{Store, s};
use dispatch_core::mcp::{
    api::types::{AgentArea, AgentKeyRequest},
    data::settle,
};
use dispatch_core::testing as common;
use dispatch_cortex::{
    self as cortex,
    discovery::{CollectionRequest, Scope},
    scorecard::{self, Capture, Request},
};
use dispatch_scorecard::ScorecardStore;
use serde_json::{Value, json};

/// Scorecard, and the Cortex collector whose scorecard it keeps.
fn install() {
    common::install(&[&cortex::COLLECTOR], &[&dispatch_scorecard::FEATURE]);
}

fn request(week: &str) -> Request {
    Request::parse(
        &json!({"collection":"scorecard","week":week,"station":"TST1",
        "timezone":"America/Los_Angeles","dspName":"Fixture Delivery",
        "dspAbbreviation":"FXTR"}),
    )
    .unwrap()
    .unwrap()
}
fn scope(request: &Request) -> Scope {
    match request.scope_request() {
        CollectionRequest::Discover(discovery) => {
            discovery.scope("area-fixture", "company-fixture").unwrap()
        }
        CollectionRequest::Scoped(scope) => scope,
    }
}
/// A DSP with a station and an enabled Cortex connection.
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    common::set_dsp(&db, &id, "Fixture Delivery", "America/Los_Angeles").unwrap();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"FXTR","setupRequired":false}),
    )
    .unwrap();
    common::ready_connection(&db, &id, cortex::PROVIDER).unwrap();
    (root, db, id)
}
/// Queues `week` and publishes `capture` as that job's outcome.
fn publish(db: &Store, id: &str, key: &str, week: &str, capture: &Capture) -> String {
    let job = db.enqueue_scorecard(id, None, key, Some(week)).unwrap();
    let job = s(&job, "id").to_owned();
    let queued = request(week);
    db.publish_scorecard(id, &job, capture, &scope(&queued))
        .unwrap();
    job
}

#[test]
fn a_posted_week_is_published_into_one_table_per_dataset_with_its_keys() {
    install();
    let (_root, db, id) = ready();
    let capture = scorecard::fixture(&request("2026-W38")).unwrap();
    let job = publish(&db, &id, "first", "2026-W38", &capture);
    let queued = db.job_row(&job, Some(&id)).unwrap();
    assert_eq!(queued.kind.as_str(), "cortex.scorecard.collect");
    let storage = db.scorecard_db(&id).unwrap();
    let publication = storage
        .one(
            "SELECT * FROM scorecard_publications WHERE job_id=?",
            [&job],
        )
        .unwrap()
        .unwrap();
    assert_eq!(publication["active"], 1);
    assert_eq!(publication["week"], "2026-W38");
    assert_eq!(publication["row_count"], capture.row_count() as i64);
    for dataset in scorecard::DATASETS {
        let captured = capture
            .datasets
            .iter()
            .find(|d| d.id == dataset.id)
            .unwrap();
        let count = storage
            .count(
                &format!(
                    "SELECT count(*) FROM {} WHERE publication_id=? AND week='2026-W38'",
                    dataset.table
                ),
                [s(&publication, "id")],
            )
            .unwrap();
        assert_eq!(count as usize, captured.rows.len(), "{}", dataset.table);
    }
    // Rows keep every field, and the keys are read from them.
    let returns = storage
        .all(
            "SELECT tracking_id,transporter_id,impact,json_extract(row,'$.rts_reason_code') reason \
             FROM returns_to_station WHERE publication_id=? ORDER BY row_index",
            [s(&publication, "id")],
        )
        .unwrap();
    assert_eq!(
        returns,
        vec![
            json!({"tracking_id":"TBA000000000001","transporter_id":"driver-1","impact":1,"reason":"BUSINESS CLOSED"}),
            json!({"tracking_id":"TBA000000000002","transporter_id":"driver-2","impact":0,"reason":"CUSTOMER UNAVAILABLE"}),
        ]
    );
    assert_eq!(
        storage
            .all("SELECT data_date,event_id,impact FROM safety_events", [])
            .unwrap(),
        vec![json!({"data_date":"2026-09-19","event_id":"90000001","impact":1})]
    );
    let weeks = db.scorecard_weeks(&id).unwrap();
    assert_eq!(weeks.station, "TST1");
    assert_eq!(weeks.weeks.len(), 1);
    let week = &weeks.weeks[0];
    assert!(week.posted);
    let published = week.publication.as_ref().unwrap();
    assert_eq!(published.row_count, capture.row_count());
    assert_eq!(published.dsp_code, "FXTR");
    assert_eq!(
        published
            .datasets
            .iter()
            .find(|d| d.table == "pickup_failures")
            .unwrap()
            .rows,
        0
    );
    // The same request queues the same job again.
    assert_eq!(
        s(
            &db.enqueue_scorecard(&id, None, "first", Some("2026-W38"))
                .unwrap(),
            "id"
        ),
        job
    );
    // The address each dataset was read at is kept beside its rows.
    assert_eq!(
        storage
            .count(
                "SELECT count(*) FROM scorecard_sources WHERE source='api'",
                []
            )
            .unwrap() as usize,
        scorecard::DATASETS.len()
    );
}

#[test]
fn the_next_job_reads_at_the_address_the_last_publication_read() {
    install();
    let (_root, db, id) = ready();
    assert_eq!(db.scorecard_address(&id, "TST1").unwrap(), None);
    let mut older = scorecard::fixture(&request("2026-W30")).unwrap();
    for dataset in &mut older.datasets {
        dataset.source_url = dataset.source_url.replace("/v1/", "/v0/");
    }
    publish(&db, &id, "older", "2026-W30", &older);
    let mut newer = scorecard::fixture(&request("2026-W31")).unwrap();
    newer.started_at += 1000;
    newer.finished_at += 1000;
    publish(&db, &id, "newer", "2026-W31", &newer);
    let (url, company) = db.scorecard_address(&id, "TST1").unwrap().unwrap();
    assert!(url.contains("/performance/api/v1/getData?"), "{url}");
    assert_eq!(company, "company-fixture");
    // Another station has no address yet.
    assert_eq!(db.scorecard_address(&id, "TST2").unwrap(), None);
}

#[test]
fn collecting_a_week_again_supersedes_its_publication_and_keeps_the_history() {
    install();
    let (_root, db, id) = ready();
    let capture = scorecard::fixture(&request("2026-W37")).unwrap();
    let first = publish(&db, &id, "one", "2026-W37", &capture);
    let mut again = capture.clone();
    again.datasets[1].rows.pop();
    again.started_at += 1000;
    again.finished_at += 2000;
    let second = publish(&db, &id, "two", "2026-W37", &again);
    let storage = db.scorecard_db(&id).unwrap();
    assert_eq!(
        storage
            .all(
                "SELECT job_id,active FROM scorecard_publications ORDER BY collected_at",
                []
            )
            .unwrap(),
        vec![
            json!({"job_id":first,"active":0}),
            json!({"job_id":second,"active":1})
        ]
    );
    let weeks = db.scorecard_weeks(&id).unwrap();
    assert_eq!(
        weeks.weeks[0].publication.as_ref().unwrap().row_count,
        again.row_count()
    );
    // Rows of the superseded publication are still there, and go with it.
    assert_eq!(
        storage
            .count("SELECT count(*) FROM returns_to_station", [])
            .unwrap(),
        3
    );
    storage
        .exec("DELETE FROM scorecard_publications WHERE active=0", [])
        .unwrap();
    assert_eq!(
        storage
            .count("SELECT count(*) FROM returns_to_station", [])
            .unwrap(),
        1
    );
}

#[test]
fn week_counts_use_capture_metadata_and_recover_missing_legacy_dataset_counts() {
    install();
    let (_root, db, id) = ready();
    let legacy = scorecard::fixture(&request("2026-W36")).unwrap();
    let legacy_job = publish(&db, &id, "legacy", "2026-W36", &legacy);
    let old = scorecard::fixture(&request("2026-W37")).unwrap();
    publish(&db, &id, "old", "2026-W37", &old);
    let mut current = old.clone();
    current.datasets[1].rows.pop();
    current.started_at += 1000;
    current.finished_at += 2000;
    publish(&db, &id, "current", "2026-W37", &current);
    let storage = db.scorecard_db(&id).unwrap();
    // Migration 0002 adopted old publications without source metadata. A
    // partially missing catalog also falls back only for its missing dataset.
    storage
        .exec(
            "DELETE FROM scorecard_sources WHERE publication_id IN \
             (SELECT id FROM scorecard_publications WHERE job_id=?)",
            [&legacy_job],
        )
        .unwrap();
    storage
        .exec(
            "DELETE FROM scorecard_sources WHERE dataset='pickup_failures'",
            [],
        )
        .unwrap();
    let weeks = db.scorecard_weeks(&id).unwrap();
    assert_eq!(
        weeks
            .weeks
            .iter()
            .map(|w| w.week.as_str())
            .collect::<Vec<_>>(),
        ["2026-W37", "2026-W36"]
    );
    for (week, capture) in weeks.weeks.iter().zip([&current, &legacy]) {
        let publication = week.publication.as_ref().unwrap();
        assert_eq!(publication.datasets.len(), scorecard::DATASETS.len());
        for (dataset, count) in scorecard::DATASETS.iter().zip(&publication.datasets) {
            let captured = capture
                .datasets
                .iter()
                .find(|d| d.id == dataset.id)
                .unwrap();
            assert_eq!(count.id, dataset.id);
            assert_eq!(
                count.rows,
                captured.rows.len(),
                "{} {}",
                week.week,
                dataset.id
            );
        }
    }
}

#[test]
fn a_week_not_posted_yet_is_noted_without_a_publication() {
    install();
    let (_root, db, id) = ready();
    let mut capture = scorecard::fixture(&request("2026-W36")).unwrap();
    for dataset in &mut capture.datasets {
        dataset.rows.clear();
    }
    capture.posted = false;
    let job = publish(&db, &id, "empty", "2026-W36", &capture);
    let storage = db.scorecard_db(&id).unwrap();
    assert!(
        storage
            .one(
                "SELECT id FROM scorecard_publications WHERE job_id=?",
                [&job]
            )
            .unwrap()
            .is_none()
    );
    let weeks = db.scorecard_weeks(&id).unwrap();
    assert_eq!(weeks.weeks.len(), 1);
    assert!(!weeks.weeks[0].posted);
    assert!(weeks.weeks[0].publication.is_none());
    // A capture that claims a posted week without the DSP's own row is refused.
    let mut wrong = capture.clone();
    wrong.posted = true;
    assert!(wrong.validate(&request("2026-W36")).is_err());
}

#[test]
fn publication_rechecks_the_discovered_company_and_pinned_dsp_code() {
    install();
    let (_root, db, id) = ready();
    let week = "2026-W38";
    let job = db
        .enqueue_scorecard(&id, None, "scope", Some(week))
        .unwrap();
    let job = s(&job, "id").to_owned();
    let queued = request(week);
    let expected = scope(&queued);
    let mut capture = scorecard::fixture(&queued).unwrap();
    capture.company_id = "company-foreign".into();
    assert!(
        db.publish_scorecard(&id, &job, &capture, &expected)
            .is_err()
    );
    capture.company_id = expected.provider.clone();
    capture.dsp_code = "FOREIGN".into();
    assert!(
        db.publish_scorecard(&id, &job, &capture, &expected)
            .is_err()
    );
    assert_eq!(
        db.scorecard_db(&id)
            .unwrap()
            .count("SELECT count(*) FROM scorecard_publications", [])
            .unwrap(),
        0
    );
    capture.dsp_code = queued.dsp_abbreviation.clone();
    db.publish_scorecard(&id, &job, &capture, &expected)
        .unwrap();
}

#[test]
fn legacy_unverified_publications_are_quarantined_until_a_bound_recollection() {
    install();
    let (_root, db, id) = ready();
    let week = db.scorecard_weeks(&id).unwrap().latest_week;
    let storage = db.scorecard_db(&id).unwrap();
    storage
        .exec(
            "INSERT INTO scorecard_publications(id,job_id,week,station,company_id,dsp_code,\
             started_at,collected_at,active,row_count,adapter_version) VALUES \
             ('legacy','legacy-job',?,'TST1','company-foreign','FOREIGN',\
             '2026-01-01T00:00:00Z','2026-01-01T00:01:00Z',1,1,1)",
            [&week],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO scorecard_sources(publication_id,dataset,source,url,row_count) VALUES \
             ('legacy','dsp_station_weekly_quality','api','https://example.test/foreign',1)",
            [],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO dsp_quality(publication_id,row_index,week,row) VALUES \
             ('legacy',0,?,'{\"dsp_final_tier\":\"FOREIGN\"}')",
            [&week],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO scorecard_weeks(week,station,checked_at,posted,publication_id) VALUES \
             (?,'TST1','2026-01-01T00:01:00Z',1,'legacy')",
            [&week],
        )
        .unwrap();

    assert_eq!(db.scorecard_jobs(&id).unwrap().len(), 1);
    assert!(db.scorecard_weeks(&id).unwrap().weeks.is_empty());
    assert_eq!(db.scorecard_address(&id, "TST1").unwrap(), None);

    let mut empty = scorecard::fixture(&request(&week)).unwrap();
    empty.posted = false;
    for dataset in &mut empty.datasets {
        dataset.rows.clear();
    }
    publish(&db, &id, "verified-empty-after-legacy", &week, &empty);
    let checked = db.scorecard_weeks(&id).unwrap();
    assert_eq!(checked.weeks.len(), 1);
    assert!(!checked.weeks[0].posted);
    assert!(checked.weeks[0].publication.is_none());

    let capture = scorecard::fixture(&request(&week)).unwrap();
    publish(&db, &id, "verified-after-legacy", &week, &capture);
    assert_eq!(
        storage
            .all(
                "SELECT company_id,active,scope_verified FROM scorecard_publications \
                 ORDER BY company_id",
                [],
            )
            .unwrap(),
        vec![
            json!({"company_id":"company-fixture","active":1,"scope_verified":1}),
            json!({"company_id":"company-foreign","active":0,"scope_verified":0}),
        ]
    );
    let visible = db.scorecard_weeks(&id).unwrap();
    assert_eq!(visible.weeks.len(), 1);
    assert_eq!(
        visible.weeks[0].publication.as_ref().unwrap().dsp_code,
        "FXTR"
    );
}

#[test]
fn a_schedule_queues_the_latest_week_until_it_is_published() {
    install();
    let (_root, db, id) = ready();
    let latest = db.scorecard_weeks(&id).unwrap().latest_week;
    let jobs = db.scorecard_jobs(&id).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, format!("scorecard:{latest}"));
    assert_eq!(
        jobs[0].1,
        json!({"collection":"scorecard","week":latest,"station":"TST1"})
    );
    // Not posted yet: asked again once the recheck interval has passed.
    let mut empty = scorecard::fixture(&request(&latest)).unwrap();
    for dataset in &mut empty.datasets {
        dataset.rows.clear();
    }
    empty.posted = false;
    publish(&db, &id, "empty", &latest, &empty);
    assert!(db.scorecard_jobs(&id).unwrap().is_empty());
    let storage = db.scorecard_db(&id).unwrap();
    storage
        .exec(
            "UPDATE scorecard_weeks SET checked_at='2020-01-01T00:00:00.000Z' WHERE week=?",
            [&latest],
        )
        .unwrap();
    assert_eq!(db.scorecard_jobs(&id).unwrap().len(), 1);
    // Published: nothing more to ask for, and never an older week.
    publish(
        &db,
        &id,
        "posted",
        &latest,
        &scorecard::fixture(&request(&latest)).unwrap(),
    );
    assert!(db.scorecard_jobs(&id).unwrap().is_empty());
    assert_eq!(
        storage
            .all("SELECT DISTINCT week FROM scorecard_weeks", [])
            .unwrap(),
        vec![json!({"week":latest})]
    );
    // Without a station there is nothing to ask for.
    db.set_profile(&id, json!({"stationCode":""})).unwrap();
    assert_eq!(
        db.scorecard_jobs(&id).unwrap_err().code,
        "scorecard_station_required"
    );
    assert_eq!(
        db.enqueue_scorecard(&id, None, "none", None)
            .unwrap_err()
            .code,
        "scorecard_station_required"
    );
}

/// A build without the feature that holds the routes has no delivery addresses: feedback
/// still answers, and grouping it by address is refused as a grouping it can't give.
#[test]
fn feedback_answers_without_the_feature_that_holds_addresses() {
    install();
    let (_root, db, id) = ready();
    db.enable_all_features(&id).unwrap();
    let areas: Vec<_> = AgentArea::all().map(AgentArea::as_str).collect();
    let request = AgentKeyRequest::parse(&json!({
        "name": "feedback", "allDsps": false, "dsps": [id], "access": "read",
        "reads": {"areas": areas, "bypass": false}, "dspReads": [], "expiresAt": null,
    }))
    .unwrap();
    let key = db
        .create_agent_key(&common::platform_owner(&db), &request)
        .unwrap();
    let caller = db.authenticate_agent(&key.token, "test").unwrap();
    let state = State::new(db.config.clone()).unwrap();
    let ask =
        |query: Value| settle(dispatch_scorecard::feedback(&db, &state, &caller, &query)).unwrap();
    let (status, body) = ask(json!({"group_by": "address"}));
    assert_eq!(
        (status, body["error"].as_str()),
        (400, Some("invalid_group_by")),
        "{body}"
    );
    // With no week collected, it says so.
    let (status, body) = ask(json!({}));
    assert_eq!(
        (status, body["error"].as_str()),
        (404, Some("not_collected")),
        "{body}"
    );
}
