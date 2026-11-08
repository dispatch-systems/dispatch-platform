//! The job queue takes every kind a registered collector collects, and only those.
use dispatch_core::{
    collection::registry::Provider,
    foundation::{config::Config, crypto},
    server::startup,
};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[test]
fn only_a_kind_a_registered_collector_collects_is_queued() {
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = startup::prepare_storage(config).unwrap();
    let dsp = crypto::id("dsp").unwrap();
    // The table takes any kind; only those a collector collects are queued.
    for kind in ["cortex.collect", "unknown.collect", ""] {
        let refused = store
            .insert_job(&dsp, None, kind, kind, 1, &json!({}))
            .unwrap_err();
        assert_eq!(
            (refused.code.as_str(), refused.status),
            ("operation_failed", 500)
        );
    }
    assert_eq!(
        store.jobs.count("SELECT count(*) FROM jobs", []).unwrap(),
        0
    );
    for kind in Provider::all().flat_map(Provider::job_kinds) {
        let job = store
            .insert_job(
                &dsp,
                Some("user"),
                kind,
                kind,
                3,
                &json!({"date":"2026-10-03"}),
            )
            .unwrap();
        let row = store.job_row(&job, Some(&dsp)).unwrap();
        assert_eq!(
            (
                row.kind.as_str(),
                row.idempotency_key.as_str(),
                row.connection_revision
            ),
            (kind, kind, 3)
        );
    }
}

#[test]
fn queued_jobs_beyond_unknown_and_ineligible_pages_remain_claimable() {
    crate::install();
    let (_root, mut store) = dispatch_core::testing::store();
    store.config.browser_capacity = 2;
    let kind = Provider::all()
        .flat_map(Provider::job_kinds)
        .next()
        .unwrap();
    for (tenant, unknown) in [("unknown", true), ("waiting", false)] {
        for i in 0..250 {
            let id = store
                .insert_job(tenant, None, &format!("queued-{i}"), kind, 1, &json!({}))
                .unwrap();
            store.jobs.exec(
                "UPDATE jobs SET kind=CASE WHEN ? THEN 'future.collect' ELSE kind END,created_at='2026-01-01T00:00:00.000Z' WHERE id=?",
                rusqlite::params![unknown, id],
            ).unwrap();
        }
    }
    let first = store
        .insert_job("ready-a", None, "first", kind, 1, &json!({}))
        .unwrap();
    let same_dsp = store
        .insert_job("ready-a", None, "next", kind, 1, &json!({}))
        .unwrap();
    let second = store
        .insert_job("ready-b", None, "second", kind, 1, &json!({}))
        .unwrap();
    let third = store
        .insert_job("ready-c", None, "third", kind, 1, &json!({}))
        .unwrap();
    for (id, date) in [
        (&first, "2026-01-02"),
        (&same_dsp, "2026-01-03"),
        (&second, "2026-01-04"),
        (&third, "2026-01-05"),
    ] {
        store
            .jobs
            .exec("UPDATE jobs SET created_at=? WHERE id=?", [date, id])
            .unwrap();
    }
    let eligible = |tenant: &str, _: Provider| tenant.starts_with("ready-");
    let claimed = store
        .claim_job("worker-a", eligible)
        .unwrap()
        .expect("runnable jobs beyond the first page");
    assert_eq!(claimed.id, first);
    assert!(claimed.held_by("worker-a"));
    let claimed = store.claim_job("worker-b", eligible).unwrap().unwrap();
    assert_eq!(claimed.id, second, "one worker at a time per DSP");
    assert!(
        store.claim_job("worker-c", eligible).unwrap().is_none(),
        "browser capacity still applies"
    );
    assert_eq!(
        store.job_row(&same_dsp, None).unwrap().status.as_str(),
        "queued"
    );
    assert_eq!(
        store.job_row(&third, None).unwrap().status.as_str(),
        "queued"
    );
    assert_eq!(
        store
            .jobs
            .count(
                "SELECT count(*) FROM jobs WHERE kind='future.collect' AND status='queued'",
                []
            )
            .unwrap(),
        250
    );
}
