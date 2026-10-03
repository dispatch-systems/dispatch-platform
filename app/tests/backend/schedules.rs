//! Schedules of the collections Timecard keeps from Paycom and Cortex: what they queue,
//! when, and what stops them.
use crate::{collectors::cortex, workforce::TimecardStore};
use dispatch_core::{
    collection::schedules::{anchor, next_daily},
    db::{Store, iso, now, s},
    foundation::{config::Config, crypto},
};
use dispatch_paycom as paycom;
use rusqlite::params;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
fn ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}
fn setup() -> (tempfile::TempDir, Store, String) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let db = Store::initialize(config).unwrap();
    let id = crypto::id("dsp").unwrap();
    db.platform
        .exec(
            "INSERT INTO dsps(id,name,environment,status,timezone,created_at) \
        VALUES (?,'Schedule test','preview','provisioning','America/Chicago',?)",
            [&id, &iso()],
        )
        .unwrap();
    db.provision(&id).unwrap();
    db.enable_all_features(&id).unwrap();
    db.collector(&id, paycom::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=1", [])
        .unwrap();
    (root, db, id)
}
fn input(collection: &str) -> Value {
    json!({"name":"Collection","collection":collection,"cadence":"interval","intervalMinutes":120,"localTime":"00:00","enabled":true})
}
fn meals(db: &Store, id: &str) {
    db.collector(id, cortex::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=1", [])
        .unwrap();
    let scope = cortex::discovery::Scope {
        date: "2026-01-10".into(),
        station: "DEMO1".into(),
        service_area_id: "area-demo".into(),
        provider: "provider-demo".into(),
        timezone: "America/Chicago".into(),
    };
    db.publish_meals(id, "seed-meals", &cortex::meals::fixture(&scope), &scope)
        .unwrap();
}
fn due(db: &Store, id: &str, key: &str, deadline: &str) {
    db.dsp(id)
        .unwrap()
        .exec(
            "UPDATE collection_schedules SET next_run=? WHERE id=?",
            [deadline, key],
        )
        .unwrap();
}
#[test]
fn resuming_preview_preserves_the_saved_interval_anchor() {
    let (_root, db, id) = setup();
    let mut value = input("paycom");
    value["intervalMinutes"] = json!(300);
    value["enabled"] = json!(false);
    let saved = db.save_schedule(&id, None, &value).unwrap();
    let key = saved.id.as_str();
    // Simulate an interval created on a prior day. Five hours does not
    // divide into a day, so anchoring anew would change its future runs.
    let old_anchor = anchor("00:00", "America/Chicago", now()).unwrap() - 86400000;
    db.dsp(&id)
        .unwrap()
        .exec(
            "UPDATE collection_schedules SET anchor=? WHERE id=?",
            params![old_anchor, key],
        )
        .unwrap();
    let preview = db
        .preview_schedule(
            &id,
            &json!({"scheduleId":key,"cadence":"interval",
        "intervalMinutes":300,"localTime":"00:00"}),
        )
        .unwrap();
    let resumed = db
        .enable_schedule(&id, key, &json!({"revision":1,"enabled":true}))
        .unwrap();
    assert_eq!(Some(&preview.next_run), resumed.next_run.as_ref());
    assert_eq!((ms(&preview.next_run) - old_anchor) % (300 * 60000), 0);
    let changed = db
        .preview_schedule(
            &id,
            &json!({"scheduleId":key,"cadence":"daily","intervalMinutes":null,
        "localTime":"06:00"}),
        )
        .unwrap();
    assert_eq!(
        changed.next_run,
        next_daily("06:00", "America/Chicago", now()).unwrap()
    );
}
#[test]
fn each_collection_target_queues_the_correct_jobs_and_replay_is_idempotent() {
    for collection in ["paycom", "meal_break", "both"] {
        let (_root, db, id) = setup();
        meals(&db, &id);
        let row = db.save_schedule(&id, None, &input(collection)).unwrap();
        let key = row.id.as_str();
        let deadline = "2026-01-01T00:00:00.000Z";
        due(&db, &id, key, deadline);
        let next = db.schedule_due(&id).unwrap().unwrap();
        assert!(next > now());
        let jobs = db.jobs.all("SELECT * FROM jobs", []).unwrap();
        assert_eq!(jobs.len(), if collection == "both" { 2 } else { 1 });
        assert_eq!(
            jobs.iter()
                .filter(|j| s(j, "kind") == "paycom.collect")
                .count(),
            usize::from(collection != "meal_break")
        );
        for job in jobs
            .iter()
            .filter(|j| s(j, "kind") == "cortex.meal_breaks.collect")
        {
            let request: Value = serde_json::from_str(s(job, "request")).unwrap();
            assert_eq!(request["station"], "DEMO1");
            assert_eq!(
                request["date"],
                chrono::Utc::now()
                    .with_timezone(&chrono_tz::America::Chicago)
                    .format("%Y-%m-%d")
                    .to_string()
            );
        }
        due(&db, &id, key, deadline); // Crash after the batch committed, before advancing the schedule.
        db.schedule_due(&id).unwrap();
        assert_eq!(
            db.jobs.all("SELECT id FROM jobs", []).unwrap().len(),
            jobs.len()
        );
        assert_eq!(db.collection_schedule(&id, key).unwrap().last_error, None);
    }
}
#[test]
fn blocked_and_overlapping_schedules_never_start_half_a_batch() {
    let (_root, db, id) = setup();
    meals(&db, &id);
    let first = db.save_schedule(&id, None, &input("paycom")).unwrap().id;
    let second = db.save_schedule(&id, None, &input("both")).unwrap().id;
    let stored = |key: &str| db.collection_schedule(&id, key).unwrap();
    due(&db, &id, &first, "2026-01-01T00:00:00.000Z");
    due(&db, &id, &second, "2026-01-02T00:00:00.000Z");
    db.schedule_due(&id).unwrap();
    assert_eq!(db.jobs.all("SELECT id FROM jobs", []).unwrap().len(), 1);
    assert_eq!(
        stored(&second).last_error.as_deref(),
        Some("sync_in_progress")
    );
    db.jobs
        .exec("UPDATE jobs SET status='succeeded'", [])
        .unwrap();
    db.collector(&id, cortex::PROVIDER)
        .unwrap()
        .exec("UPDATE connections SET enabled=0", [])
        .unwrap();
    db.schedule_due(&id).unwrap();
    assert_eq!(db.jobs.all("SELECT id FROM jobs", []).unwrap().len(), 1);
    assert_eq!(
        stored(&second).last_error.as_deref(),
        Some("schedule_meals_required")
    );
    db.pause_provider_schedules(&id, cortex::PROVIDER).unwrap();
    assert!(!stored(&second).enabled);
    assert!(stored(&first).enabled);
}
#[test]
fn timezone_changes_recompute_deadlines_and_stale_edits_are_rejected() {
    let (_root, db, id) = setup();
    let mut value = input("paycom");
    value["cadence"] = json!("daily");
    value["intervalMinutes"] = Value::Null;
    value["localTime"] = json!("06:00");
    let row = db.save_schedule(&id, None, &value).unwrap();
    db.retime_schedules(&id, "Asia/Tokyo").unwrap();
    let changed = db.collection_schedule(&id, &row.id).unwrap();
    assert_eq!(
        changed.next_run,
        Some(next_daily("06:00", "Asia/Tokyo", now()).unwrap())
    );
    value["revision"] = json!(1);
    assert_eq!(
        db.save_schedule(&id, Some(&row.id), &value)
            .unwrap_err()
            .code,
        "schedule_changed"
    );
}
