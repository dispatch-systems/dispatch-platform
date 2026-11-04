//! `upgrade-check`: a restored copy of a platform, started under this binary as `serve` starts
//! it before it listens, reporting what it migrated and what failed. The platform itself is
//! never started.
use dispatch_core::{
    db::Store,
    server::operations::{self, Lock},
    testing,
};
use dispatch_paycom::PROVIDER as PAYCOM;
use dispatch_routes::RoutesStore;
use serde_json::{Value, json};

/// Whether `db` holds the index Timecard's third Paycom migration adds to `dsp`'s Paycom data.
fn indexed(db: &Store, dsp: &str) -> bool {
    db.collector(dsp, PAYCOM)
        .unwrap()
        .one(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND name='employees_by_code'",
            [],
        )
        .unwrap()
        .is_some()
}

#[test]
fn a_restored_copy_starts_as_serve_would_reporting_its_migrations_and_failed_carry_overs() {
    crate::install();
    let (root, store, dsp) = testing::bootstrapped();
    // The platform as a release before left it: without Timecard's Paycom index, and with a
    // Route Data window for Settings to carry over, in a table that can't be read.
    store
        .collector(&dsp, PAYCOM)
        .unwrap()
        .0
        .execute_batch("DROP INDEX employees_by_code; DELETE FROM schema_migrations WHERE id=3")
        .unwrap();
    store
        .routes_db(&dsp)
        .unwrap()
        .0
        .execute_batch(
            "DROP TABLE route_retention; CREATE TABLE route_retention(id INTEGER PRIMARY KEY)",
        )
        .unwrap();
    testing::before_settings(&store, &dsp).unwrap();
    // A server ran on it, as one runs on Production.
    drop(Lock::acquire(root.path()).unwrap());
    let external = tempfile::tempdir().unwrap();
    let backup = external.path().join("backup");
    operations::backup(&store.config, &backup).unwrap();
    let copy = external.path().join("copy");
    operations::restore(&backup, &copy).unwrap();

    let (report, started) = operations::upgrade_check(&store.config, &copy).unwrap();
    started.unwrap();
    assert_eq!(
        (
            &report["databases"],
            &report["migrations"],
            &report["stopped"]
        ),
        (&json!(1), &json!({"paycom": 1}), &Value::Null)
    );
    let failed = report["carryOverFailures"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(
        (&failed[0]["dspId"], &failed[0]["section"]),
        (&json!(dsp), &json!("routes.retention"))
    );
    let mut config = store.config.clone();
    config.root = copy;
    assert!(indexed(
        &Store::open(config, store.key.clone()).unwrap(),
        &dsp
    ));
    // The platform a server ran on is refused, and stays as it was.
    let refused = operations::upgrade_check(&store.config, root.path()).unwrap_err();
    assert_eq!(refused.code, "upgrade_check_restored_copy_required");
    assert!(!indexed(&store, &dsp));
}
