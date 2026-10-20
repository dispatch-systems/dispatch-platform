//! Weekly Scorecard storage: publication, supersession, weeks not posted, where the next
//! job reads and what a schedule queues.
use dispatch_core::db::{Store, s};
use dispatch_core::manifest::Section;
use dispatch_core::settings;
use dispatch_core::testing as common;
use dispatch_cortex::{
    self as cortex,
    discovery::{CollectionRequest, Scope},
    weekly_scorecard::{self, Capture, Request},
};
use dispatch_weekly_scorecard::WeeklyScorecardStore;
use serde_json::{Value, json};

/// Weekly Scorecard, and the Cortex collector whose scorecard it keeps.
fn install() {
    common::install(
        &[&cortex::COLLECTOR],
        &[&dispatch_weekly_scorecard::FEATURE],
    );
}

fn request(week: &str) -> Request {
    Request::parse(
        &json!({"collection":"weekly_scorecard","week":week,"station":"TST1",
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
/// Which weeks its schedules recheck, as Weekly Scorecard declares it.
fn collection() -> &'static Section {
    let mut sections = dispatch_weekly_scorecard::FEATURE.settings.iter();
    sections
        .find(|s| s.id == "weekly_scorecard.collection")
        .unwrap()
}
/// Queues `week` and publishes `capture` as that job's outcome.
fn publish(db: &Store, id: &str, key: &str, week: &str, capture: &Capture) -> String {
    let job = db
        .enqueue_weekly_scorecard(id, None, key, Some(week))
        .unwrap();
    let job = s(&job, "id").to_owned();
    let queued = request(week);
    db.publish_weekly_scorecard(id, &job, capture, &scope(&queued))
        .unwrap();
    job
}

#[test]
fn a_posted_week_is_published_into_one_table_per_dataset_with_its_keys() {
    install();
    let (_root, db, id) = ready();
    let capture = weekly_scorecard::fixture(&request("2026-W38")).unwrap();
    let job = publish(&db, &id, "first", "2026-W38", &capture);
    let queued = db.job_row(&job, Some(&id)).unwrap();
    assert_eq!(queued.kind.as_str(), "cortex.weekly_scorecard.collect");
    let storage = db.weekly_scorecard_db(&id).unwrap();
    let publication = storage
        .one(
            "SELECT * FROM weekly_scorecard_publications WHERE job_id=?",
            [&job],
        )
        .unwrap()
        .unwrap();
    assert_eq!(publication["active"], 1);
    assert_eq!(publication["week"], "2026-W38");
    assert_eq!(publication["row_count"], capture.row_count() as i64);
    for dataset in weekly_scorecard::DATASETS {
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
    let weeks = db.weekly_scorecard_weeks(&id).unwrap();
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
            &db.enqueue_weekly_scorecard(&id, None, "first", Some("2026-W38"))
                .unwrap(),
            "id"
        ),
        job
    );
    // The address each dataset was read at is kept beside its rows.
    assert_eq!(
        storage
            .count(
                "SELECT count(*) FROM weekly_scorecard_sources WHERE source='api'",
                []
            )
            .unwrap() as usize,
        weekly_scorecard::DATASETS.len()
    );
}

#[test]
fn the_next_job_reads_at_the_address_the_last_publication_read() {
    install();
    let (_root, db, id) = ready();
    assert_eq!(db.weekly_scorecard_address(&id, "TST1").unwrap(), None);
    let mut older = weekly_scorecard::fixture(&request("2026-W30")).unwrap();
    for dataset in &mut older.datasets {
        dataset.source_url = dataset.source_url.replace("/v1/", "/v0/");
    }
    publish(&db, &id, "older", "2026-W30", &older);
    let mut newer = weekly_scorecard::fixture(&request("2026-W31")).unwrap();
    newer.started_at += 1000;
    newer.finished_at += 1000;
    publish(&db, &id, "newer", "2026-W31", &newer);
    let (url, company) = db.weekly_scorecard_address(&id, "TST1").unwrap().unwrap();
    assert!(url.contains("/performance/api/v1/getData?"), "{url}");
    assert_eq!(company, "company-fixture");
    // Another station has no address yet.
    assert_eq!(db.weekly_scorecard_address(&id, "TST2").unwrap(), None);
}

#[test]
fn collecting_a_week_again_supersedes_its_publication_and_keeps_the_history() {
    install();
    let (_root, db, id) = ready();
    let capture = weekly_scorecard::fixture(&request("2026-W37")).unwrap();
    let first = publish(&db, &id, "one", "2026-W37", &capture);
    let mut again = capture.clone();
    let mut late = again.datasets[1].rows[0].clone();
    late["tracking_id"] = json!("TBA000000000009");
    again.datasets[1].rows.push(late);
    again.started_at += 1000;
    again.finished_at += 2000;
    let second = publish(&db, &id, "two", "2026-W37", &again);
    let storage = db.weekly_scorecard_db(&id).unwrap();
    assert_eq!(
        storage
            .all(
                "SELECT job_id,active FROM weekly_scorecard_publications ORDER BY collected_at",
                []
            )
            .unwrap(),
        vec![
            json!({"job_id":first,"active":0}),
            json!({"job_id":second,"active":1})
        ]
    );
    let weeks = db.weekly_scorecard_weeks(&id).unwrap();
    assert_eq!(
        weeks.weeks[0].publication.as_ref().unwrap().row_count,
        again.row_count()
    );
    // Rows of the superseded publication are still there, and go with it.
    assert_eq!(
        storage
            .count("SELECT count(*) FROM returns_to_station", [])
            .unwrap(),
        5
    );
    storage
        .exec(
            "DELETE FROM weekly_scorecard_publications WHERE active=0",
            [],
        )
        .unwrap();
    assert_eq!(
        storage
            .count("SELECT count(*) FROM returns_to_station", [])
            .unwrap(),
        3
    );
}

// A week answered with fewer rows than it holds, as Cortex answers while it republishes a
// week, is kept beside the published week until a read at least half an hour later sees the
// same; a week that grew publishes at once.
#[test]
fn a_week_that_shrank_waits_for_a_second_read() {
    install();
    let (_root, db, id) = ready();
    let full = weekly_scorecard::fixture(&request("2026-W36")).unwrap();
    let first = publish(&db, &id, "full", "2026-W36", &full);
    let mut shrunk = full.clone();
    shrunk.datasets[1].rows.pop();
    let storage = db.weekly_scorecard_db(&id).unwrap();
    let active = || {
        storage
            .one(
                "SELECT job_id FROM weekly_scorecard_publications WHERE active=1",
                [],
            )
            .unwrap()
            .unwrap()["job_id"]
            .clone()
    };
    publish(&db, &id, "shrunk-1", "2026-W36", &shrunk);
    assert_eq!(active(), json!(first));
    publish(&db, &id, "shrunk-2", "2026-W36", &shrunk);
    assert_eq!(active(), json!(first));
    assert_eq!(
        db.weekly_scorecard_weeks(&id).unwrap().weeks[0]
            .publication
            .as_ref()
            .unwrap()
            .row_count,
        full.row_count()
    );
    // Half an hour later the same counts are what the week now is: the held reads came
    // after the published one, and longer ago than that.
    let ago = |minutes: i64| dispatch_core::db::at(dispatch_core::db::now() - minutes * 60_000);
    storage
        .exec(
            "UPDATE weekly_scorecard_publications SET collected_at=CASE active WHEN 1 THEN ? ELSE ? END",
            [ago(120), ago(31)],
        )
        .unwrap();
    let confirmed = publish(&db, &id, "shrunk-3", "2026-W36", &shrunk);
    assert_eq!(active(), json!(confirmed));
    let mut grown = shrunk.clone();
    grown.datasets[1].rows = full.datasets[1].rows.clone();
    let grew = publish(&db, &id, "grown", "2026-W36", &grown);
    assert_eq!(active(), json!(grew));
}

#[test]
fn week_counts_use_capture_metadata_and_recover_missing_legacy_dataset_counts() {
    install();
    let (_root, db, id) = ready();
    let legacy = weekly_scorecard::fixture(&request("2026-W36")).unwrap();
    let legacy_job = publish(&db, &id, "legacy", "2026-W36", &legacy);
    let old = weekly_scorecard::fixture(&request("2026-W37")).unwrap();
    publish(&db, &id, "old", "2026-W37", &old);
    let mut current = old.clone();
    let mut late = current.datasets[1].rows[0].clone();
    late["tracking_id"] = json!("TBA000000000009");
    current.datasets[1].rows.push(late);
    current.started_at += 1000;
    current.finished_at += 2000;
    publish(&db, &id, "current", "2026-W37", &current);
    let storage = db.weekly_scorecard_db(&id).unwrap();
    // Migration 0002 adopted old publications without source metadata. A
    // partially missing catalog also falls back only for its missing dataset.
    storage
        .exec(
            "DELETE FROM weekly_scorecard_sources WHERE publication_id IN \
             (SELECT id FROM weekly_scorecard_publications WHERE job_id=?)",
            [&legacy_job],
        )
        .unwrap();
    storage
        .exec(
            "DELETE FROM weekly_scorecard_sources WHERE dataset='pickup_failures'",
            [],
        )
        .unwrap();
    let weeks = db.weekly_scorecard_weeks(&id).unwrap();
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
        assert_eq!(publication.datasets.len(), weekly_scorecard::DATASETS.len());
        for (dataset, count) in weekly_scorecard::DATASETS.iter().zip(&publication.datasets) {
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
    let mut capture = weekly_scorecard::fixture(&request("2026-W36")).unwrap();
    for dataset in &mut capture.datasets {
        dataset.rows.clear();
    }
    capture.posted = false;
    let job = publish(&db, &id, "empty", "2026-W36", &capture);
    let storage = db.weekly_scorecard_db(&id).unwrap();
    assert!(
        storage
            .one(
                "SELECT id FROM weekly_scorecard_publications WHERE job_id=?",
                [&job]
            )
            .unwrap()
            .is_none()
    );
    let weeks = db.weekly_scorecard_weeks(&id).unwrap();
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
        .enqueue_weekly_scorecard(&id, None, "scope", Some(week))
        .unwrap();
    let job = s(&job, "id").to_owned();
    let queued = request(week);
    let expected = scope(&queued);
    let mut capture = weekly_scorecard::fixture(&queued).unwrap();
    capture.company_id = "company-foreign".into();
    assert!(
        db.publish_weekly_scorecard(&id, &job, &capture, &expected)
            .is_err()
    );
    capture.company_id = expected.provider.clone();
    capture.dsp_code = "FOREIGN".into();
    assert!(
        db.publish_weekly_scorecard(&id, &job, &capture, &expected)
            .is_err()
    );
    assert_eq!(
        db.weekly_scorecard_db(&id)
            .unwrap()
            .count("SELECT count(*) FROM weekly_scorecard_publications", [])
            .unwrap(),
        0
    );
    capture.dsp_code = queued.dsp_abbreviation.clone();
    db.publish_weekly_scorecard(&id, &job, &capture, &expected)
        .unwrap();
}

#[test]
fn legacy_unverified_publications_are_quarantined_until_a_bound_recollection() {
    install();
    let (_root, db, id) = ready();
    let week = db.weekly_scorecard_weeks(&id).unwrap().latest_week;
    let storage = db.weekly_scorecard_db(&id).unwrap();
    storage
        .exec(
            "INSERT INTO weekly_scorecard_publications(id,job_id,week,station,company_id,dsp_code,\
             started_at,collected_at,active,row_count,adapter_version) VALUES \
             ('legacy','legacy-job',?,'TST1','company-foreign','FOREIGN',\
             '2026-01-01T00:00:00Z','2026-01-01T00:01:00Z',1,1,1)",
            [&week],
        )
        .unwrap();
    storage
        .exec(
            "INSERT INTO weekly_scorecard_sources(publication_id,dataset,source,url,row_count) VALUES \
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
            "INSERT INTO weekly_scorecard_weeks(week,station,checked_at,posted,publication_id) VALUES \
             (?,'TST1','2026-01-01T00:01:00Z',1,'legacy')",
            [&week],
        )
        .unwrap();

    assert_eq!(db.weekly_scorecard_jobs(&id).unwrap().len(), 1);
    assert!(db.weekly_scorecard_weeks(&id).unwrap().weeks.is_empty());
    assert_eq!(db.weekly_scorecard_address(&id, "TST1").unwrap(), None);

    let mut empty = weekly_scorecard::fixture(&request(&week)).unwrap();
    empty.posted = false;
    for dataset in &mut empty.datasets {
        dataset.rows.clear();
    }
    publish(&db, &id, "verified-empty-after-legacy", &week, &empty);
    let checked = db.weekly_scorecard_weeks(&id).unwrap();
    assert_eq!(checked.weeks.len(), 1);
    assert!(!checked.weeks[0].posted);
    assert!(checked.weeks[0].publication.is_none());

    let capture = weekly_scorecard::fixture(&request(&week)).unwrap();
    publish(&db, &id, "verified-after-legacy", &week, &capture);
    assert_eq!(
        storage
            .all(
                "SELECT company_id,active,scope_verified FROM weekly_scorecard_publications \
                 ORDER BY company_id",
                [],
            )
            .unwrap(),
        vec![
            json!({"company_id":"company-fixture","active":1,"scope_verified":1}),
            json!({"company_id":"company-foreign","active":0,"scope_verified":0}),
        ]
    );
    let visible = db.weekly_scorecard_weeks(&id).unwrap();
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
    let latest = db.weekly_scorecard_weeks(&id).unwrap().latest_week;
    let jobs = db.weekly_scorecard_jobs(&id).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, format!("weekly_scorecard:{latest}"));
    assert_eq!(
        jobs[0].1,
        json!({"collection":"weekly_scorecard","week":latest,"station":"TST1"})
    );
    // Not posted yet: asked again once the recheck interval has passed.
    let mut empty = weekly_scorecard::fixture(&request(&latest)).unwrap();
    for dataset in &mut empty.datasets {
        dataset.rows.clear();
    }
    empty.posted = false;
    publish(&db, &id, "empty", &latest, &empty);
    assert!(db.weekly_scorecard_jobs(&id).unwrap().is_empty());
    let storage = db.weekly_scorecard_db(&id).unwrap();
    storage
        .exec(
            "UPDATE weekly_scorecard_weeks SET checked_at='2020-01-01T00:00:00.000Z' WHERE week=?",
            [&latest],
        )
        .unwrap();
    assert_eq!(db.weekly_scorecard_jobs(&id).unwrap().len(), 1);
    // A fresh publication waits until its refresh interval; the default examines only this week.
    publish(
        &db,
        &id,
        "posted",
        &latest,
        &weekly_scorecard::fixture(&request(&latest)).unwrap(),
    );
    assert!(db.weekly_scorecard_jobs(&id).unwrap().is_empty());
    assert_eq!(
        storage
            .all("SELECT DISTINCT week FROM weekly_scorecard_weeks", [])
            .unwrap(),
        vec![json!({"week":latest})]
    );
    let actor = common::a_user(&db);
    let policy = json!({"lookback_weeks": 2, "refresh_hours": 12});
    settings::save(&db, &id, &actor, collection(), &policy, 0, false).unwrap();
    let jobs = db.weekly_scorecard_jobs(&id).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_ne!(jobs[0].1["week"], latest);
    storage
        .exec(
            "UPDATE weekly_scorecard_weeks SET checked_at='2020-01-01T00:00:00.000Z' WHERE week=?",
            [&latest],
        )
        .unwrap();
    assert_eq!(db.weekly_scorecard_jobs(&id).unwrap().len(), 2);
    // Without a station there is nothing to ask for.
    db.set_profile(&id, json!({"stationCode":""})).unwrap();
    assert_eq!(
        db.weekly_scorecard_jobs(&id).unwrap_err().code,
        "weekly_scorecard_station_required"
    );
    assert_eq!(
        db.enqueue_weekly_scorecard(&id, None, "none", None)
            .unwrap_err()
            .code,
        "weekly_scorecard_station_required"
    );
}

#[test]
fn a_policy_chosen_before_settings_kept_it_moves_in_once_for_its_own_dsp() {
    install();
    let (_root, db, id) = ready();
    let actor = common::a_user(&db);
    let other = db.new_dsp("Other DSP", "UTC", &actor, false).unwrap().id;
    // Each DSP's policy as the release before kept it, in its own database, and its settings
    // as they were then.
    for (dsp, lookback) in [(&id, 2), (&other, 6)] {
        db.weekly_scorecard_db(dsp)
            .unwrap()
            .set(
                "collection_policy",
                &json!({"lookbackWeeks": lookback, "refreshHours": 12}),
            )
            .unwrap();
        common::before_settings(&db, dsp).unwrap();
    }
    let upgrade = dispatch_weekly_scorecard::FEATURE.upgrade_storage.unwrap();
    let policy = |dsp: &str| settings::read::<Value>(&db, dsp, collection()).unwrap();
    upgrade(&db, &id).unwrap();
    assert_eq!(
        policy(&id),
        json!({"lookback_weeks": 2, "refresh_hours": 12})
    );
    assert_eq!(
        policy(&other),
        json!({"lookback_weeks": 1, "refresh_hours": 20})
    );
    upgrade(&db, &other).unwrap();
    assert_eq!(
        policy(&other),
        json!({"lookback_weeks": 6, "refresh_hours": 12})
    );
    assert_eq!(db.weekly_scorecard_jobs(&id).unwrap().len(), 2);
    // Changed in Settings since, it isn't moved again; and the move itself recorded nothing.
    let changed = json!({"lookback_weeks": 4});
    settings::save(&db, &id, &actor, collection(), &changed, 1, false).unwrap();
    upgrade(&db, &id).unwrap();
    assert_eq!(
        policy(&id),
        json!({"lookback_weeks": 4, "refresh_hours": 12})
    );
    assert!(common::audit_actions(&db, &other, "settings.").is_empty());
}
