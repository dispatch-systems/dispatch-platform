//! Daily isolation, publication safety and scheduling.
use dispatch_core::{
    db::{Store, at, now, s},
    manifest::Section,
    settings, testing,
};
use dispatch_cortex::{
    self as cortex,
    daily_performance::{self, Days, Request},
    discovery::CollectionRequest,
};
use dispatch_daily_performance::DailyPerformanceStore;
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
/// Which dates its schedules recheck, as Daily Performance declares it.
fn collection() -> &'static Section {
    let mut sections = dispatch_daily_performance::FEATURE.settings.iter();
    sections
        .find(|s| s.id == "daily_performance.collection")
        .unwrap()
}
/// The fixture's day `date`, published by a job of its own, which it answers.
fn publish(store: &Store, dsp: &str, date: &str, key: &str) -> daily_performance::Capture {
    publish_with(store, dsp, date, key, |_| {}).1
}
/// The fixture's day `date` as `change` leaves it, published by a job of its own: the job,
/// and the day.
fn publish_with(
    store: &Store,
    dsp: &str,
    date: &str,
    key: &str,
    change: impl Fn(&mut daily_performance::Capture),
) -> (String, daily_performance::Capture) {
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
    let mut days = daily_performance::fixture(&request).unwrap();
    change(&mut days.days[0]);
    store
        .publish_daily_performance(dsp, job, &days, &scope)
        .unwrap();
    (job.to_owned(), days.days[0].clone())
}
#[test]
fn unknown_stored_dataset_returns_an_error_without_panicking() {
    let (_root, store, dsp) = ready();
    publish(&store, &dsp, "2026-10-03", "unknown-dataset");
    store
        .daily_performance_db(&dsp)
        .unwrap()
        .exec(
            "UPDATE daily_datasets SET dataset='retired_dataset' WHERE dataset='driver_quality'",
            [],
        )
        .unwrap();
    let error = store.daily_performance_days(&dsp).unwrap_err();
    assert_eq!(error.code, "daily_performance_unknown_stored_dataset");
    assert_eq!(error.status, 503);
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
        dates: vec!["2026-10-03".into()],
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
            &Days {
                days: vec![first.clone()],
            },
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
// A job over several days publishes each as that day's own active publication, and
// publishing it again, as a retried publication would, adds nothing; a job queued before
// jobs read several days still binds to its one.
#[test]
fn a_job_publishes_each_of_its_days_once() {
    let (_root, store, dsp) = ready();
    let queued = store
        .enqueue_daily_performance(&dsp, None, "days", "2026-10-01", "2026-10-03")
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
    let days = daily_performance::fixture(&request).unwrap();
    for _ in 0..2 {
        store
            .publish_daily_performance(&dsp, job, &days, &scope)
            .unwrap();
    }
    let data = store.daily_performance_db(&dsp).unwrap();
    assert_eq!(
        data.count(
            "SELECT count(*) FROM daily_publications WHERE job_id=? AND active=1",
            [job]
        )
        .unwrap(),
        3
    );
    assert_eq!(
        data.count("SELECT count(*) FROM daily_rows", []).unwrap(),
        days.row_count() as i64
    );
    let published: Vec<String> = store
        .daily_performance_days(&dsp)
        .unwrap()
        .days
        .iter()
        .map(|day| day.date.clone())
        .collect();
    assert_eq!(published, ["2026-10-03", "2026-10-02", "2026-10-01"]);
    let legacy = keeper
        .bind(
            &store,
            &dsp,
            &json!({"collection":"daily_performance","date":"2026-10-02","station":"TST1"}),
        )
        .unwrap();
    assert_eq!(legacy["dates"], json!(["2026-10-02"]));
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
    let days = daily_performance::fixture(&request).unwrap();
    for (field, value) in [
        ("data_date", "2026-10-02"),
        ("company_id", "foreign"),
        ("station_code", "OTHER"),
    ] {
        let mut wrong = days.clone();
        wrong.days[0].datasets[0].rows[0][field] = json!(value);
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
fn schedules_refresh_configured_dates_and_manual_backfills_are_bounded_and_idempotent() {
    let (_root, store, dsp) = ready();
    let actor = testing::a_user(&store);
    let policy = json!({"lookback_days": 3, "refresh_hours": 12});
    settings::save(&store, &dsp, &actor, collection(), &policy, 0, false).unwrap();
    // One job reads every date due, oldest first.
    let due = |store: &Store| -> Vec<String> {
        let jobs = store.daily_performance_jobs(&dsp).unwrap();
        assert_eq!(jobs.len(), 1);
        serde_json::from_value(jobs[0].1["dates"].clone()).unwrap()
    };
    let dates = due(&store);
    assert_eq!(dates.len(), 3);
    assert!(dates.windows(2).all(|pair| pair[0] < pair[1]));
    publish(&store, &dsp, &dates[2], "recent");
    assert_eq!(due(&store), dates[..2]);
    // Collected 15 hours ago, past the policy's 12 though within the default 20, it's due.
    store
        .daily_performance_db(&dsp)
        .unwrap()
        .exec(
            "UPDATE daily_publications SET collected_at=?",
            [at(now() - 15 * 3_600_000)],
        )
        .unwrap();
    assert_eq!(due(&store), dates);
    // A backfill's range is one job.
    let first = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first.as_array().unwrap().len(), 1);
    let request: Value = serde_json::from_str(
        &store
            .job_row(s(&first[0], "id"), Some(&dsp))
            .unwrap()
            .request,
    )
    .unwrap();
    assert_eq!(
        request["dates"],
        json!(["2026-10-01", "2026-10-02", "2026-10-03"])
    );
    let again = store
        .enqueue_daily_performance(&dsp, None, "backfill", "2026-10-01", "2026-10-03")
        .unwrap();
    assert_eq!(first, again);
    assert!(
        store
            .enqueue_daily_performance(&dsp, None, "huge", "2026-01-01", "2026-10-03")
            .is_err()
    );
}
#[test]
fn a_policy_chosen_before_settings_kept_it_moves_in_once_for_its_own_dsp() {
    let (_root, store, dsp) = ready();
    let actor = testing::a_user(&store);
    let other = store.new_dsp("Other DSP", "UTC", &actor, false).unwrap().id;
    // Each DSP's policy as the release before kept it, in its own database, and its settings
    // as they were then.
    for (id, lookback) in [(&dsp, 3), (&other, 14)] {
        store
            .daily_performance_db(id)
            .unwrap()
            .set(
                "collection_policy",
                &json!({"lookbackDays": lookback, "refreshHours": 12}),
            )
            .unwrap();
        testing::before_settings(&store, id).unwrap();
    }
    let upgrade = dispatch_daily_performance::FEATURE.upgrade_storage.unwrap();
    let policy = |id: &str| settings::read::<Value>(&store, id, collection()).unwrap();
    upgrade(&store, &dsp).unwrap();
    assert_eq!(
        policy(&dsp),
        json!({"lookback_days": 3, "refresh_hours": 12})
    );
    assert_eq!(
        policy(&other),
        json!({"lookback_days": 7, "refresh_hours": 20})
    );
    upgrade(&store, &other).unwrap();
    assert_eq!(
        policy(&other),
        json!({"lookback_days": 14, "refresh_hours": 12})
    );
    assert_eq!(
        store.daily_performance_jobs(&dsp).unwrap()[0].1["dates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Changed in Settings since, it isn't moved again; and the move itself recorded nothing.
    let changed = json!({"lookback_days": 5});
    settings::save(&store, &dsp, &actor, collection(), &changed, 1, false).unwrap();
    upgrade(&store, &dsp).unwrap();
    assert_eq!(
        policy(&dsp),
        json!({"lookback_days": 5, "refresh_hours": 12})
    );
    assert!(testing::audit_actions(&store, &other, "settings.").is_empty());
}
// The rebuild that lets one job publish several days keeps every publication, dataset and
// row, the children still reference their publication, and a job may publish another day
// but not the same day twice.
#[test]
fn the_days_per_job_migration_keeps_every_row_and_allows_a_job_its_days() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db.execute_batch(include_str!(
        "../../../migrations/daily_performance/0001_baseline.sql"
    ))
    .unwrap();
    db.execute_batch(
        "INSERT INTO daily_publications VALUES ('p1','job-1','2026-10-03','TST1','company-1','FXTR',\
         '2026-10-04T00:00:00Z','2026-10-04T00:00:10Z',1,1); \
         INSERT INTO daily_datasets VALUES ('p1','driver_quality','https://example.test/q',1,'observed'); \
         INSERT INTO daily_rows VALUES ('p1','driver_quality',0,'2026-10-03','driver-1',NULL,NULL,NULL,'{\"delivered\":1}');",
    )
    .unwrap();
    db.execute_batch(include_str!(
        "../../../migrations/daily_performance/0002_days_per_job.sql"
    ))
    .unwrap();
    let count = |sql: &str| -> i64 { db.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count("SELECT count(*) FROM daily_publications WHERE id='p1' AND active=1"),
        1
    );
    assert_eq!(
        count("SELECT count(*) FROM daily_datasets WHERE publication_id='p1'"),
        1
    );
    assert_eq!(
        count(
            "SELECT count(*) FROM daily_rows WHERE publication_id='p1' AND json_extract(row,'$.delivered')=1"
        ),
        1
    );
    for child in ["daily_datasets", "daily_rows"] {
        let parent: String = db
            .query_row(
                &format!("SELECT \"table\" FROM pragma_foreign_key_list('{child}')"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent, "daily_publications", "{child}");
    }
    let publish = |id: &str, date: &str| {
        db.execute(
            "INSERT INTO daily_publications VALUES (?,'job-1',?,'TST1','company-1','FXTR',\
             '2026-10-04T00:00:00Z','2026-10-04T00:00:10Z',0,0)",
            [id, date],
        )
    };
    publish("p2", "2026-10-02").unwrap();
    assert!(publish("p3", "2026-10-03").is_err());
    assert!(
        db.execute(
            "INSERT INTO daily_datasets VALUES ('missing','driver_quality','u',0,'unconfirmed')",
            []
        )
        .is_err()
    );
}
// A finished day answered with fewer rows than it holds, as Cortex answers while it
// republishes a day, is kept beside the published day until a read at least half an hour
// later sees the same; a day that grew, and the day not over yet, publish at once.
#[test]
fn a_finished_day_that_shrank_waits_for_a_second_read() {
    let (_root, store, dsp) = ready();
    let today = dispatch_core::foundation::clock::today(chrono_tz::America::Los_Angeles);
    let yesterday = (today - chrono::Duration::days(1)).to_string();
    let date = yesterday.as_str();
    let shrink = |capture: &mut daily_performance::Capture| {
        capture.datasets[0].rows.pop();
    };
    let (full, _) = publish_with(&store, &dsp, date, "full", |_| {});
    let data = store.daily_performance_db(&dsp).unwrap();
    let active = |date: &str| {
        data.one(
            "SELECT job_id FROM daily_publications WHERE date=? AND active=1",
            [date],
        )
        .unwrap()
        .unwrap()["job_id"]
            .clone()
    };
    let (held, _) = publish_with(&store, &dsp, date, "shrunk-1", shrink);
    assert_eq!(active(date), json!(full));
    // A held read keeps its counts alone.
    let kept = |job: &str, table: &str| {
        data.count(
            &format!(
                "SELECT count(*) FROM {table} WHERE publication_id IN \
                 (SELECT id FROM daily_publications WHERE job_id=?)"
            ),
            [job],
        )
        .unwrap()
    };
    assert_eq!(
        (kept(&held, "daily_datasets"), kept(&held, "daily_rows")),
        (22, 0)
    );
    publish_with(&store, &dsp, date, "shrunk-2", shrink);
    assert_eq!(active(date), json!(full));
    // Half an hour later the same counts are what the day now is: the held reads came after
    // the published one, and longer ago than that. Its published day, older than the
    // refresh, is due again meanwhile.
    data.exec(
        "UPDATE daily_publications SET collected_at=CASE active WHEN 1 THEN ? ELSE ? END",
        [at(now() - 21 * 3_600_000), at(now() - 31 * 60_000)],
    )
    .unwrap();
    let due = store.daily_performance_jobs(&dsp).unwrap();
    assert!(due[0].1["dates"].as_array().unwrap().contains(&json!(date)));
    let (confirmed, _) = publish_with(&store, &dsp, date, "shrunk-3", shrink);
    assert_eq!(active(date), json!(confirmed));
    // A smaller count than the one confirmed is held again; more rows publish at once.
    publish_with(&store, &dsp, date, "smaller", |capture| {
        capture.datasets[0].rows.truncate(1)
    });
    assert_eq!(active(date), json!(confirmed));
    let (grown, _) = publish_with(&store, &dsp, date, "grown", |_| {});
    assert_eq!(active(date), json!(grown));
    // Reads held before the day was published again confirm nothing about it.
    publish_with(&store, &dsp, date, "after", shrink);
    assert_eq!(active(date), json!(grown));
    // The day not over yet changes as it goes, and is never held.
    let today = today.to_string();
    publish_with(&store, &dsp, &today, "today", |_| {});
    let (shrunk_today, _) = publish_with(&store, &dsp, &today, "today-shrunk", shrink);
    assert_eq!(active(&today), json!(shrunk_today));
}
// A dataset gone empty, as it reads while Cortex answers nothing, waits twelve hours for a
// second read to agree before the day publishes it.
#[test]
fn a_finished_day_gone_empty_waits_half_a_day() {
    let (_root, store, dsp) = ready();
    let today = dispatch_core::foundation::clock::today(chrono_tz::America::Los_Angeles);
    let yesterday = (today - chrono::Duration::days(1)).to_string();
    let empty = |capture: &mut daily_performance::Capture| capture.datasets[0].rows.clear();
    let (full, _) = publish_with(&store, &dsp, &yesterday, "full", |_| {});
    let data = store.daily_performance_db(&dsp).unwrap();
    let active = || {
        data.one("SELECT job_id FROM daily_publications WHERE active=1", [])
            .unwrap()
            .unwrap()["job_id"]
            .clone()
    };
    let age = |held_minutes: i64| {
        data.exec(
            "UPDATE daily_publications SET collected_at=CASE active WHEN 1 THEN ? ELSE ? END",
            [
                at(now() - 24 * 3_600_000),
                at(now() - held_minutes * 60_000),
            ],
        )
        .unwrap()
    };
    publish_with(&store, &dsp, &yesterday, "empty-1", empty);
    age(31);
    publish_with(&store, &dsp, &yesterday, "empty-2", empty);
    assert_eq!(active(), json!(full));
    age(13 * 60);
    let (confirmed, _) = publish_with(&store, &dsp, &yesterday, "empty-3", empty);
    assert_eq!(active(), json!(confirmed));
}
