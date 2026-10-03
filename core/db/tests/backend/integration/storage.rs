#[path = "../../support/common.rs"]
mod common;
use common::{bootstrapped, seeded, store};
use dispatch_backend::{
    crypto,
    db::{self, Store},
    operations,
};
use serde_json::json;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn core_storage_is_bound_to_its_dsp_on_every_open() {
    let (_root, db, id) = bootstrapped();
    let core = db.dsp(&id).unwrap();
    let identity = core
        .all("SELECT dsp_id,provider,source FROM storage_identity", [])
        .unwrap();
    assert_eq!(
        identity,
        vec![json!({"dsp_id":id,"provider":"dispatch","source":"dispatch-v1"})]
    );
    core.exec(
        "UPDATE storage_identity SET dsp_id='dsp_00000000000000000000000000000000'",
        [],
    )
    .unwrap();
    drop(core);
    assert_eq!(
        db.dsp(&id).err().unwrap().code,
        "dsp_storage_identity_mismatch"
    );
}

#[test]
fn recorded_core_identity_is_not_recreated_when_missing() {
    let (_root, db, id) = bootstrapped();
    let core = db.dsp(&id).unwrap();
    core.exec("DELETE FROM storage_identity", []).unwrap();
    drop(core);
    let config = db.config.clone();
    drop(db);
    assert_eq!(
        Store::initialize(config).err().unwrap().code,
        "dsp_storage_identity_mismatch"
    );
}

#[test]
fn legacy_core_storage_is_adopted_once_without_losing_data() {
    let (_root, db, id) = bootstrapped();
    let core = db.dsp(&id).unwrap();
    core.set("legacy.witness", &json!({"kept":true})).unwrap();
    core.0
        .execute_batch("DROP TABLE storage_identity; DELETE FROM schema_migrations WHERE id=5;")
        .unwrap();
    drop(core);
    let config = db.config.clone();
    drop(db);

    let reopened = Store::initialize(config).unwrap();
    let core = reopened.dsp(&id).unwrap();
    assert_eq!(
        core.setting("legacy.witness", serde_json::Value::Null)
            .unwrap(),
        json!({"kept":true})
    );
    assert_eq!(
        core.all("SELECT dsp_id,provider,source FROM storage_identity", [])
            .unwrap(),
        vec![json!({"dsp_id":id,"provider":"dispatch","source":"dispatch-v1"})]
    );
    assert_eq!(
        core.0
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn startup_rejects_cross_dsp_core_database_substitution() {
    let (_root, db) = seeded();
    let ids: Vec<String> = db
        .platform
        .all(
            "SELECT id FROM dsps WHERE name IN ('Northline Logistics','Summit Delivery') \
             ORDER BY name",
            [],
        )
        .unwrap()
        .into_iter()
        .map(|row| db::s(&row, "id").to_owned())
        .collect();
    let first = db.area(&ids[0], "data").unwrap().join("dispatch.sqlite");
    let second = db.area(&ids[1], "data").unwrap().join("dispatch.sqlite");
    let swap = db.config.root.join("core-swap.sqlite");
    let config = db.config.clone();
    drop(db);
    std::fs::rename(&first, &swap).unwrap();
    std::fs::rename(&second, &first).unwrap();
    std::fs::rename(&swap, &second).unwrap();

    assert_eq!(
        Store::initialize(config).err().unwrap().code,
        "dsp_storage_identity_mismatch"
    );
}

#[test]
fn startup_rejects_provider_database_as_core_storage() {
    let (_root, db) = seeded();
    let id = db
        .platform
        .one("SELECT id FROM dsps WHERE name='Northline Logistics'", [])
        .unwrap()
        .map(|row| db::s(&row, "id").to_owned())
        .unwrap();
    let data = db.area(&id, "data").unwrap();
    let core = data.join("dispatch.sqlite");
    let provider = data.join("paycom/paycom.sqlite");
    let config = db.config.clone();
    drop(db);
    std::fs::copy(provider, core).unwrap();

    assert_eq!(
        Store::initialize(config).err().unwrap().code,
        "dsp_storage_identity_mismatch"
    );
}

#[test]
fn startup_removes_owned_browseros_runs_and_rejects_unknown_entries() {
    let (_root, db) = store();
    let runs = db::private_dir(&db.config.environment_root().join("browser-runs")).unwrap();
    for prefix in ["run", "browseros"] {
        db::private_dir(&runs.join(crypto::id(prefix).unwrap())).unwrap();
    }
    operations::clean_browser_runs(&db.config).unwrap();
    assert_eq!(std::fs::read_dir(&runs).unwrap().count(), 0);
    let unknown = runs.join("unrecognized");
    db::private_dir(&unknown).unwrap();
    assert_eq!(
        operations::clean_browser_runs(&db.config).unwrap_err().code,
        "unexpected_browser_run"
    );
    assert!(unknown.exists());
}

#[test]
fn private_storage_rejects_links_and_world_readable_files() {
    let (root, db) = store();
    let first = root.path().join("private");
    db::write_private(&first, b"private").unwrap();
    let link = root.path().join("link");
    symlink(&first, &link).unwrap();
    assert!(db::private_file(&link, false).is_err());
    let hard = root.path().join("hard");
    std::fs::hard_link(&first, &hard).unwrap();
    assert!(db::private_file(&first, false).is_err());
    std::fs::remove_file(hard).unwrap();
    std::fs::set_permissions(&first, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(db::private_file(&first, false).is_err());
    assert!(db.area("../escape", "data").is_err());
}

#[test]
fn legacy_account_database_is_rejected_without_changing_its_schema() {
    let (_root, db) = store();
    let config = db.config.clone();
    db.platform
        .0
        .pragma_update(None, "user_version", 2)
        .unwrap();
    drop(db);
    let error = Store::initialize(config.clone()).err().unwrap();
    assert_eq!(error.code, "incompatible_database");
    let old = rusqlite::Connection::open(config.platform().join("accounts.sqlite")).unwrap();
    assert_eq!(
        old.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn essential_background_failure_stops_readiness_and_normal_shutdown_is_clean() {
    use dispatch_backend::{Error, supervise};
    let (stop, receiver) = tokio::sync::watch::channel(false);
    assert!(
        supervise(async { Err(Error::new("scheduler_failed", 500)) }, stop)
            .await
            .is_err()
    );
    assert!(*receiver.borrow());
    let (stop, receiver) = tokio::sync::watch::channel(false);
    assert!(
        supervise(async { panic!("fixture panic") }, stop)
            .await
            .is_err()
    );
    assert!(*receiver.borrow());
    let (stop, _) = tokio::sync::watch::channel(true);
    assert!(supervise(async { Ok(()) }, stop).await.is_ok());
}
