//! Jobs and schedules running Timecard's collections from Paycom and Cortex: the queue's
//! limits, the facts an outcome records, and what the Timecard's switch stops.
#[path = "../../../../core/db/tests/support/common.rs"]
mod common;
use common::{seeded, store};
use dispatch_backend::{
    collectors::{cortex, paycom},
    db::{self, s},
    jobs::JobFacts,
    workforce::TimecardStore,
};
use serde_json::json;

#[test]
fn queue_limits_and_authority_are_checked_again_before_publication() {
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
    db.collector(id, dispatch_backend::collectors::paycom::PROVIDER)
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

#[test]
fn schedule_deadlines_track_changes_and_due_ticks_are_idempotent() {
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
        .save_collection_schedule(
            id,
            None,
            &json!({"name":"Morning","collection":"paycom","cadence":"daily","intervalMinutes":null,"localTime":"06:00","enabled":true}),
        )
        .unwrap();
    let key = s(&schedule, "id");
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
    db.enable_collection_schedule(
        id,
        key,
        &json!({"revision":schedule["revision"],"enabled":false}),
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

#[test]
fn nothing_collects_for_a_dsp_without_the_timecard() {
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
    db.save_collection_schedule(
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
        .set_feature(id, dispatch_backend::features::schedules(), false, actor)
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
    db.set_feature(id, dispatch_backend::features::schedules(), true, actor)
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
