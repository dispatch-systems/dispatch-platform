//! Jobs and schedules running the features' collections from Paycom and Cortex: the queue's
//! limits, the lists each page reads, the facts an outcome records, and what the Timecard's
//! switch stops.
// A build that leaves features out drops the tests that need them, and what only they use.
#![cfg_attr(not(feature = "default"), allow(dead_code, unused_imports))]
use common::{seeded, store};
use dispatch_core::testing as common;
use dispatch_core::{
    collection::jobs::JobFacts,
    db::{self, Store, s},
};
use dispatch_cortex as cortex;
#[cfg(feature = "dvic")]
use dispatch_dvic::DvicStore;
use dispatch_paycom as paycom;
#[cfg(feature = "timecard")]
use dispatch_timecard::TimecardStore;
use serde_json::json;

/// A DSP with a DVIC station and an enabled Cortex connection.
fn dvic_ready() -> (tempfile::TempDir, Store, String) {
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

#[cfg(feature = "timecard")]
#[test]
fn queue_limits_and_authority_are_checked_again_before_publication() {
    dispatch_backend::install();
    let (_root, db) = seeded();
    let tenant = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let id = s(&tenant, "id");
    let user = db
        .platform
        .one("SELECT id FROM users WHERE platform_owner=1", [])
        .unwrap()
        .unwrap();
    let actor = s(&user, "id");
    db.enqueue_timecards(id, Some(actor), "request-0").unwrap();
    assert_eq!(
        db.enqueue_timecards(id, Some(actor), "another-manual")
            .unwrap_err()
            .code,
        "sync_in_progress"
    );
    // The underlying queue still bounds internal batches independently of the manual lock.
    for i in 1..5 {
        db.enqueue_timecards(id, None, &format!("request-{i}"))
            .unwrap();
    }
    assert_eq!(
        db.enqueue_timecards(id, None, "overflow")
            .unwrap_err()
            .status,
        429
    );
    let job = db.claim_job("worker", |_, _| true).unwrap().unwrap();
    let jid = job.id.as_str();
    db.guard(jid, "worker").unwrap();
    assert!(db.claim_job("second", |_, _| true).unwrap().is_none());
    db.platform
        .exec("UPDATE users SET status='disabled' WHERE id=?", [actor])
        .unwrap();
    assert_eq!(
        db.guard(jid, "worker").unwrap_err().code,
        "permission_denied"
    );
    db.platform
        .exec("UPDATE users SET status='active' WHERE id=?", [actor])
        .unwrap();
    db.collector(id, dispatch_paycom::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET revision=revision+1", [])
        .unwrap();
    assert_eq!(
        db.guard(jid, "worker").unwrap_err().code,
        "connection_changed"
    );
    db.cancel(jid, id).unwrap();
    assert_eq!(db.guard(jid, "worker").unwrap_err().code, "job_cancelled");
}

#[test]
fn collection_outcomes_record_their_schedule_provider_date_and_duration() {
    dispatch_backend::install();
    let (_root, db) = store();
    let started = db::at(db::now() - 108_000);
    let job = JobFacts {
        idempotency_key: "manual",
        provider: Some(paycom::PROVIDER),
        attempt: 1,
        max_attempts: 1,
        request: "{\"date\":\"2026-09-18\"}",
        started_at: Some(&started),
    };
    let (schedule, facts) = db.job_facts("missing", &job);
    assert!(schedule.is_none());
    assert_eq!(
        facts,
        [
            ("provider", None, Some("paycom".to_owned())),
            ("date", None, Some("2026-09-18".to_owned())),
            ("duration", None, Some("108".to_owned())),
        ]
    );
    let job = JobFacts {
        idempotency_key: "schedule:gone:2026:flex:0",
        provider: Some(cortex::PROVIDER),
        request: "{}",
        started_at: None,
        ..job
    };
    let (schedule, facts) = db.job_facts("missing", &job);
    assert!(schedule.is_none());
    assert_eq!(facts, [("provider", None, Some("cortex".to_owned()))]);
}

#[cfg(feature = "timecard")]
#[test]
fn schedule_deadlines_track_changes_and_due_ticks_are_idempotent() {
    dispatch_backend::install();
    let (_root, db) = seeded();
    let dsp = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let id = s(&dsp, "id");
    assert!(
        !db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(d, _)| d == id)
    );
    let schedule = db
        .save_schedule(
            id,
            None,
            &json!({"name":"Morning","collection":"paycom","cadence":"daily","intervalMinutes":null,"localTime":"06:00","enabled":true}),
        )
        .unwrap();
    let key = schedule.id.as_str();
    assert!(
        db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(d, at)| d == id && *at > db::now())
    );
    db.dsp(id)
        .unwrap()
        .exec(
            "UPDATE collection_schedules SET next_run='2026-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    assert!(db.schedule_due(id).unwrap().unwrap() > db::now());
    assert!(db.schedule_due(id).unwrap().unwrap() > db::now());
    assert_eq!(db.recent_jobs(Some(id)).unwrap().len(), 1);
    db.enable_schedule(
        id,
        key,
        &json!({"revision":schedule.revision,"enabled":false}),
    )
    .unwrap();
    assert_eq!(db.schedule_due(id).unwrap(), None);
    assert!(
        !db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(d, _)| d == id)
    );
}

/// A failed attempt is queued again only for a retryable code, a core one or a collector's,
/// and only while attempts remain; any other failure, and the last attempt's, ends the job.
#[cfg(feature = "timecard")]
#[test]
fn a_retryable_failure_is_queued_again_until_the_attempts_run_out() {
    dispatch_backend::install();
    let (_root, db) = seeded();
    let tenant = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let id = s(&tenant, "id");
    let user = db
        .platform
        .one("SELECT id FROM users WHERE platform_owner=1", [])
        .unwrap()
        .unwrap();
    let actor = s(&user, "id");
    let fail = |key: &str, error: &str| {
        db.enqueue_timecards(id, Some(actor), key).unwrap();
        let job = db.claim_job("worker", |_, _| true).unwrap().unwrap();
        db.finish(&job.id, "worker", Some(error)).unwrap();
        db.job_row(&job.id, Some(id)).unwrap()
    };
    for (key, error) in [
        ("core", "browser_lost"),
        ("cortex", "cortex_source_changed"),
    ] {
        let job = fail(key, error);
        assert_eq!(job.status.as_str(), "queued", "{error}");
        assert_eq!((job.attempt, job.max_attempts), (1, 3), "{error}");
        assert_eq!(job.error.as_deref(), Some(error));
        assert!(job.completed_at.is_none() && job.available_at > db::now());
        db.cancel_jobs(dispatch_core::collection::jobs::CancelJobs::Job {
            id: &job.id,
            dsp: id,
        })
        .unwrap();
    }
    assert_eq!(fail("other", "queue_full").status.as_str(), "failed");
    // The last attempt fails whatever its code.
    db.enqueue_timecards(id, Some(actor), "last").unwrap();
    db.jobs
        .exec(
            "UPDATE jobs SET attempt=max_attempts-1 WHERE idempotency_key='last'",
            [],
        )
        .unwrap();
    let job = db.claim_job("worker", |_, _| true).unwrap().unwrap();
    db.finish(&job.id, "worker", Some("browser_lost")).unwrap();
    let job = db.job_row(&job.id, Some(id)).unwrap();
    assert_eq!((job.status.as_str(), job.attempt), ("failed", 3));
}

#[cfg(feature = "timecard")]
#[test]
fn nothing_collects_for_a_dsp_without_the_timecard() {
    dispatch_backend::install();
    let (_root, db) = seeded();
    let tenant = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .unwrap();
    let id = s(&tenant, "id");
    let user = db
        .platform
        .one("SELECT id FROM users WHERE platform_owner=1", [])
        .unwrap()
        .unwrap();
    let actor = s(&user, "id");
    db.enqueue_timecards(id, Some(actor), "request-0").unwrap();
    let job = db.claim_job("worker", |_, _| true).unwrap().unwrap();
    let jid = job.id.as_str();
    db.guard(jid, "worker").unwrap();
    db.save_schedule(
        id,
        None,
        &json!({"name":"Morning","collection":"paycom","cadence":"daily","intervalMinutes":null,"localTime":"06:00","enabled":true}),
    )
    .unwrap();
    assert!(
        db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(dsp, _)| dsp == id)
    );

    let result = db
        .set_feature(
            id,
            dispatch_core::tenancy::catalog::automation("paycom").unwrap(),
            false,
            actor,
        )
        .unwrap();
    assert_eq!(
        result
            .changed
            .iter()
            .map(|c| c.feature.as_str())
            .collect::<Vec<_>>(),
        ["timecard"]
    );
    assert_eq!(
        db.guard(jid, "worker").unwrap_err().code,
        "feature_disabled"
    );
    assert!(db.schedule_due(id).unwrap().is_none());
    assert!(
        !db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(dsp, _)| dsp == id)
    );
    // Its schedules are exactly as they were, and run again once the page is back.
    let enabled = db
        .dsp(id)
        .unwrap()
        .count(
            "SELECT count(*) FROM collection_schedules WHERE enabled=1",
            [],
        )
        .unwrap();
    db.set_feature(
        id,
        dispatch_core::tenancy::catalog::automation("paycom").unwrap(),
        true,
        actor,
    )
    .unwrap();
    assert_eq!(
        db.dsp(id)
            .unwrap()
            .count(
                "SELECT count(*) FROM collection_schedules WHERE enabled=1",
                []
            )
            .unwrap(),
        enabled
    );
    assert!(
        db.schedule_deadlines()
            .unwrap()
            .iter()
            .any(|(dsp, _)| dsp == id)
    );
    db.guard(jid, "worker").unwrap();
}

#[test]
fn listed_jobs_respect_the_cap_scope_names_and_attempt_order() {
    dispatch_backend::install();
    let (_root, db) = seeded();
    let dsps = db
        .platform
        .all("SELECT id,name FROM dsps ORDER BY id", [])
        .unwrap();
    let transaction = db.jobs.0.unchecked_transaction().unwrap();
    for index in 0..204 {
        let dsp = &dsps[index % dsps.len()];
        let job = format!("job-{index}");
        db.jobs.exec("INSERT INTO jobs(id,dsp_id,environment,kind,status,available_at,created_at,release,connection_revision,idempotency_key) VALUES (?,?,'preview','paycom.collect','succeeded',0,?,'test',1,?)",rusqlite::params![job,s(dsp,"id"),format!("2026-09-01T{index:04}"),job]).unwrap();
        for attempt in [2, 1] {
            db.jobs
                .exec(
                    "INSERT INTO job_metrics(job_id,attempt,owner,metrics) VALUES (?,?,'test',?)",
                    rusqlite::params![
                        job,
                        attempt,
                        serde_json::to_string(&dispatch_core::collection::metrics::Metrics::new(
                            &json!({"attempt":attempt})
                        ))
                        .unwrap()
                    ],
                )
                .unwrap();
        }
    }
    transaction.commit().unwrap();
    let recent = db.recent_jobs(None).unwrap();
    assert_eq!(recent.len(), 200);
    assert_eq!(recent[0].id, "job-203");
    assert_eq!(
        recent[0]
            .metrics
            .iter()
            .map(|m| m.attempt)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let dsp = &dsps[0];
    let scoped = db.recent_jobs(Some(s(dsp, "id"))).unwrap();
    assert_eq!(scoped.len(), 68);
    for row in scoped {
        assert_eq!(row.dsp_id, s(dsp, "id"));
        assert_eq!(row.dsp_name, s(dsp, "name"));
    }
}

#[cfg(feature = "dvic")]
#[test]
fn the_status_keeps_its_jobs_however_many_of_other_kinds_came_since() {
    dispatch_backend::install();
    let (_root, db, id) = dvic_ready();
    let job = db.enqueue_dvic(&id, None, "busy", None, 2).unwrap();
    let job = s(&job, "id").to_owned();
    // A busy DSP: more newer jobs of another kind than the latest 200 hold.
    let others: Vec<_> = (0..201)
        .map(|index| (format!("paycom-{index}"), db::at(db::now() + 1000 + index)))
        .collect();
    common::finished_jobs(&db, &id, "paycom.collect", &others).unwrap();
    let listed = |jobs: Vec<dispatch_core::collection::api::jobs::PublicJob>| {
        jobs.into_iter().map(|j| j.id).collect::<Vec<_>>()
    };
    assert!(!listed(db.recent_jobs(Some(&id)).unwrap()).contains(&job));
    assert_eq!(listed(db.dvic_status(&id).unwrap().jobs), [job]);
    // The other way round: the Timecard's list keeps its own jobs however many DVIC jobs
    // came since.
    let others: Vec<_> = (0..201)
        .map(|index| (format!("dvic-{index}"), db::at(db::now() + 5000 + index)))
        .collect();
    common::finished_jobs(&db, &id, "cortex.dvic.collect", &others).unwrap();
    let others = listed(db.recent_jobs_in(&id, &["paycom.collect"]).unwrap());
    assert_eq!(others.len(), 200);
    assert!(others.iter().all(|j| j.starts_with("paycom-")));
}
