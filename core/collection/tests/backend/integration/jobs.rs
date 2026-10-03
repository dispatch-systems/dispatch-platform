#[path = "../../../../db/tests/support/common.rs"]
mod common;
use common::seeded;
use dispatch_backend::{db::s, schedules};
use serde_json::json;

#[test]
fn listed_jobs_respect_the_cap_scope_names_and_attempt_order() {
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
                        serde_json::to_string(&dispatch_backend::job_metrics::Metrics::new(
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

#[test]
fn schedule_handles_dst_gaps_and_repeated_minutes() {
    let parse = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .unwrap()
            .timestamp_millis()
    };
    assert_eq!(
        schedules::next_daily("02:30", "America/Chicago", parse("2026-03-08T07:59:00Z")).unwrap(),
        "2026-03-09T07:30:00.000Z"
    );
    assert_eq!(
        schedules::next_daily("01:30", "America/Chicago", parse("2026-11-01T06:30:00Z")).unwrap(),
        "2026-11-02T07:30:00.000Z"
    );
    assert!(schedules::next_daily("25:99", "UTC", 0).is_err());
}
