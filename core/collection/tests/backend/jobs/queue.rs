use super::*;
use crate::config::Config;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

#[test]
fn only_a_kind_a_registered_collector_collects_is_queued() {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = Store::initialize(config).unwrap();
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
