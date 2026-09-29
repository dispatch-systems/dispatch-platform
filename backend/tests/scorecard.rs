//! Scorecard storage: publication of a job's weeks, supersession, weeks not posted,
//! where the next job reads, and what a schedule queues.
mod common;
use dispatch_backend::{
    collectors::Provider,
    db::{Store, s},
    scorecard::{self, Capture, Request},
};
use serde_json::{Value, json};

fn request(weeks: &[&str]) -> Request {
    Request::parse(&json!({"collection":"scorecard","weeks":weeks,"station":"TST1"}))
        .unwrap()
        .unwrap()
}
/// A DSP with a station and an enabled Cortex connection.
fn ready() -> (tempfile::TempDir, Store, String) {
    let (root, db, id) = common::bootstrapped();
    db.set_profile(
        &id,
        json!({"stationCode":"TST1","abbreviation":"NLOG","setupRequired":false}),
    )
    .unwrap();
    db.collector(&id, Provider::Cortex)
        .unwrap()
        .exec("UPDATE connections SET enabled=1,status='ready'", [])
        .unwrap();
    (root, db, id)
}
/// Queues the capture's weeks and publishes it as that job's outcome.
fn publish(db: &Store, id: &str, key: &str, capture: &Capture) -> String {
    let job = db
        .enqueue_scorecard(
            id,
            None,
            key,
            Some(&capture.weeks[0].week),
            capture.weeks.len(),
        )
        .unwrap();
    let job = s(&job, "id").to_owned();
    db.publish_scorecard(id, &job, capture).unwrap();
    job
}
/// Empties a week of a capture, as Cortex answers a week it has not posted.
fn unposted(capture: &mut Capture, index: usize) {
    for dataset in &mut capture.weeks[index].datasets {
        dataset.rows.clear();
    }
    capture.weeks[index].posted = false;
}

#[test]
fn a_posted_week_is_published_into_one_table_per_dataset_with_its_keys() {
    let (_root, db, id) = ready();
    let capture = scorecard::fixture(&request(&["2026-W38"])).unwrap();
    let job = publish(&db, &id, "first", &capture);
    let queued: Value = db.job(&job, Some(&id)).unwrap();
    assert_eq!(queued["kind"], "cortex.scorecard.collect");
    let storage = db.scorecard(&id).unwrap();
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
        let captured = capture.weeks[0]
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
            &db.enqueue_scorecard(&id, None, "first", Some("2026-W38"), 1)
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
fn a_job_publishes_each_of_its_weeks_and_notes_those_not_posted() {
    let (_root, db, id) = ready();
    let mut capture = scorecard::fixture(&request(&["2026-W38", "2026-W37", "2026-W36"])).unwrap();
    unposted(&mut capture, 1);
    let job = publish(&db, &id, "three", &capture);
    let queued: Value = db.job(&job, Some(&id)).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(s(&queued, "request")).unwrap()["weeks"],
        json!(["2026-W38", "2026-W37", "2026-W36"])
    );
    let storage = db.scorecard(&id).unwrap();
    assert_eq!(
        storage
            .all(
                "SELECT week,active,row_count FROM scorecard_publications WHERE job_id=? ORDER BY week",
                [&job]
            )
            .unwrap(),
        vec![
            json!({"week":"2026-W36","active":1,"row_count":capture.weeks[2].row_count()}),
            json!({"week":"2026-W38","active":1,"row_count":capture.weeks[0].row_count()}),
        ]
    );
    let weeks = db.scorecard_weeks(&id).unwrap();
    let posted: Vec<(String, bool)> = weeks
        .weeks
        .iter()
        .map(|w| (w.week.clone(), w.posted))
        .collect();
    assert_eq!(
        posted,
        vec![
            ("2026-W38".into(), true),
            ("2026-W37".into(), false),
            ("2026-W36".into(), true)
        ]
    );
    // Each week's rows went to its own publication; a span's address is kept for each.
    assert_eq!(
        storage
            .all(
                "SELECT p.week,q.week row_week,json_extract(q.row,'$.data_date') data_date \
                 FROM dsp_quality q JOIN scorecard_publications p ON p.id=q.publication_id ORDER BY p.week",
                []
            )
            .unwrap(),
        vec![
            json!({"week":"2026-W36","row_week":"2026-W36","data_date":"2026-W36"}),
            json!({"week":"2026-W38","row_week":"2026-W38","data_date":"2026-W38"}),
        ]
    );
    let urls: Vec<String> = storage
        .all(
            "SELECT s.url FROM scorecard_sources s JOIN scorecard_publications p ON p.id=s.publication_id \
             WHERE s.dataset='dsp_station_weekly_quality' ORDER BY p.week",
            [],
        )
        .unwrap()
        .iter()
        .map(|row| s(row, "url").to_owned())
        .collect();
    assert!(
        urls.iter()
            .all(|url| url.ends_with("from=2026-W36&to=2026-W38")),
        "{urls:?}"
    );
}

#[test]
fn the_next_job_reads_at_the_address_the_last_publication_read() {
    let (_root, db, id) = ready();
    assert_eq!(db.scorecard_address(&id, "TST1").unwrap(), None);
    let mut older = scorecard::fixture(&request(&["2026-W30"])).unwrap();
    for dataset in &mut older.weeks[0].datasets {
        dataset.source_url = dataset.source_url.replace("/v1/", "/v0/");
    }
    publish(&db, &id, "older", &older);
    let mut newer = scorecard::fixture(&request(&["2026-W31"])).unwrap();
    newer.started_at += 1000;
    newer.finished_at += 1000;
    publish(&db, &id, "newer", &newer);
    let (url, company) = db.scorecard_address(&id, "TST1").unwrap().unwrap();
    assert!(url.contains("/performance/api/v1/getData?"), "{url}");
    assert_eq!(company, "company-fixture");
    // Another station has no address yet.
    assert_eq!(db.scorecard_address(&id, "TST2").unwrap(), None);
}

#[test]
fn collecting_a_week_again_supersedes_its_publication_and_keeps_the_history() {
    let (_root, db, id) = ready();
    let capture = scorecard::fixture(&request(&["2026-W37"])).unwrap();
    let first = publish(&db, &id, "one", &capture);
    let mut again = capture.clone();
    again.weeks[0].datasets[1].rows.pop();
    again.started_at += 1000;
    again.finished_at += 2000;
    let second = publish(&db, &id, "two", &again);
    let storage = db.scorecard(&id).unwrap();
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
fn a_week_not_posted_yet_is_noted_without_a_publication() {
    let (_root, db, id) = ready();
    let mut capture = scorecard::fixture(&request(&["2026-W36"])).unwrap();
    unposted(&mut capture, 0);
    let job = publish(&db, &id, "empty", &capture);
    let storage = db.scorecard(&id).unwrap();
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
    wrong.weeks[0].posted = true;
    assert!(wrong.validate(&request(&["2026-W36"])).is_err());
}

#[test]
fn a_schedule_queues_one_job_for_the_newest_week_then_backfills_and_refreshes_within_the_limit() {
    let (_root, db, id) = ready();
    let jobs = db.scorecard_jobs(&id).unwrap();
    assert_eq!(jobs.len(), 1);
    let latest = db.scorecard_weeks(&id).unwrap().latest_week;
    assert_eq!(jobs[0].0, "scorecard");
    assert_eq!(jobs[0].1["station"], "TST1");
    let expected = scorecard::weeks_before(&latest, scorecard::MAX_WEEKS_PER_JOB - 1).unwrap();
    let weeks = |jobs: &[(String, Value)]| -> Vec<String> {
        serde_json::from_value(jobs[0].1["weeks"].clone()).unwrap()
    };
    assert_eq!(weeks(&jobs), expected);
    // Once the newest week is published and the next is checked, the run moves on
    // to the weeks it has never seen.
    let mut first = scorecard::fixture(&request(&[&latest, &expected[1]])).unwrap();
    unposted(&mut first, 1);
    publish(&db, &id, "latest", &first);
    let queued = weeks(&db.scorecard_jobs(&id).unwrap());
    assert!(!queued.contains(&latest), "{queued:?}");
    assert!(!queued.contains(&expected[1]), "{queued:?}");
    assert_eq!(queued[0], expected[2]);
    assert_eq!(queued.len(), scorecard::MAX_WEEKS_PER_JOB);
    // A publication older than the refresh interval is collected again; an older
    // unposted check is asked again after its interval.
    let storage = db.scorecard(&id).unwrap();
    storage
        .exec(
            "UPDATE scorecard_publications SET collected_at='2020-01-01T00:00:00.000Z' WHERE week=?",
            [&latest],
        )
        .unwrap();
    storage
        .exec(
            "UPDATE scorecard_weeks SET checked_at='2020-01-01T00:00:00.000Z' WHERE week=?",
            [&expected[1]],
        )
        .unwrap();
    let queued = weeks(&db.scorecard_jobs(&id).unwrap());
    assert_eq!(queued[..2], [latest.clone(), expected[1].clone()]);
    // Without a station there is nothing to ask for.
    db.set_profile(&id, json!({"stationCode":""})).unwrap();
    assert_eq!(
        db.scorecard_jobs(&id).unwrap_err().code,
        "scorecard_station_required"
    );
    assert_eq!(
        db.enqueue_scorecard(&id, None, "none", None, 1)
            .unwrap_err()
            .code,
        "scorecard_station_required"
    );
}
