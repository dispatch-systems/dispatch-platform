//! Collector storage that holds Timecard's tables: what startup does with a DSP's Paycom
//! and Cortex databases, read through the tables Timecard keeps in them.
use crate::{
    collectors::{cortex, database_path, paycom},
    config::Config,
    db::{self, Db, Store},
    operations,
    workforce::{self, TimecardStore},
};
use paycom::fixtures;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;

fn platform() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = Store::initialize(config).unwrap();
    (root, store)
}
fn pending(store: &Store) -> String {
    let id = crate::crypto::id("dsp").unwrap();
    store
        .platform
        .exec(
            "INSERT INTO \
        dsps(id,name,environment,status,timezone,created_at) VALUES \
        (?,'Collectors','preview','provisioning','UTC',?)",
            [&id, &db::iso()],
        )
        .unwrap();
    id
}
fn provisioned() -> (tempfile::TempDir, Store, String) {
    let (root, store) = platform();
    let id = pending(&store);
    store.provision(&id).unwrap();
    store
        .dsp(&id)
        .unwrap()
        .set("dsp.profile", &json!({"stationCode":"TEST"}))
        .unwrap();
    let paycom = store.collector(&id, paycom::PROVIDER).unwrap();
    paycom
        .exec(
            "UPDATE connections SET enabled=1,status='ready',revision=7",
            [],
        )
        .unwrap();
    paycom
        .set(
            "paycom.preferences",
            &json!({"revision":3,"values":workforce::defaults(),"history":[]}),
        )
        .unwrap();
    drop(paycom);
    store
        .publish_timecards(&id, &fixtures::fixture("UTC").unwrap())
        .unwrap();
    (root, store, id)
}
fn snapshot(db: &Db) -> Value {
    let mut out = json!({});
    for table in ["connections", "publications", "employees", "timecards"] {
        out[table] = json!(
            db.all(&format!("SELECT * FROM {table} ORDER BY 1,2"), [])
                .unwrap()
        );
    }
    out["settings"] = json!(
        db.all(
            "SELECT * FROM settings WHERE key GLOB 'paycom.*' ORDER BY key",
            []
        )
        .unwrap()
    );
    out
}

#[test]
fn provider_records_stay_apart_from_core_settings_and_survive_reopening() {
    let (root, store, id) = provisioned();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    assert_eq!(before["employees"].as_array().unwrap().len(), 12);
    let core = store.dsp(&id).unwrap();
    for table in ["connections", "publications", "employees", "timecards"] {
        assert!(
            core.one(&format!("SELECT count(*) FROM {table}"), [])
                .is_err()
        );
    }
    assert_eq!(
        core.setting("dsp.profile", Value::Null).unwrap()["stationCode"],
        "TEST"
    );
    assert_eq!(
        core.setting("paycom.preferences", Value::Null).unwrap(),
        Value::Null
    );
    let provider = store.collector(&id, paycom::PROVIDER).unwrap();
    assert_eq!(
        provider.setting("dsp.profile", Value::Null).unwrap(),
        Value::Null
    );
    // A database an older binary made, from before links were retained and
    // before migrations were recorded, gains the tables on startup.
    provider
        .0
        .execute_batch("DROP TABLE timecard_sources; DROP TABLE schema_migrations;")
        .unwrap();
    store
        .collector(&id, cortex::PROVIDER)
        .unwrap()
        .0
        .execute_batch("DROP TABLE meal_sources; DROP TABLE schema_migrations;")
        .unwrap();
    drop(provider);
    drop(core);
    store.open_collectors(&id).unwrap();
    for (provider, table) in [
        (paycom::PROVIDER, "timecard_sources"),
        (cortex::PROVIDER, "meal_sources"),
    ] {
        store
            .collector(&id, provider)
            .unwrap()
            .one(&format!("SELECT count(*) FROM {table}"), [])
            .unwrap();
    }
    assert_eq!(
        snapshot(&store.collector(&id, paycom::PROVIDER).unwrap()),
        before
    );
    let path = database_path(&root.path().join("dsps").join(&id), paycom::PROVIDER).unwrap();
    assert!(path.ends_with("data/paycom/paycom.sqlite"));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let reopened = Store::open(store.config.clone(), store.key.clone()).unwrap();
    assert_eq!(
        snapshot(&reopened.collector(&id, paycom::PROVIDER).unwrap()),
        before
    );
    let mut next = fixtures::fixture("UTC").unwrap();
    next["employees"][0]["name"] = json!("New Collection");
    reopened.publish_timecards(&id, &next).unwrap();
    assert_eq!(
        reopened
            .employee_timecard(&id, "E001", None)
            .unwrap()
            .employee
            .name,
        "New Collection"
    );
}
#[test]
fn startup_opens_suspended_dsps_from_a_restored_backup() {
    let (_root, store, id) = provisioned();
    store
        .platform
        .exec("UPDATE dsps SET status='suspended' WHERE id=?", [&id])
        .unwrap();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    store
        .collector(&id, paycom::PROVIDER)
        .unwrap()
        .0
        .execute_batch("DROP TABLE timecard_sources; DROP TABLE schema_migrations;")
        .unwrap();
    let external = tempfile::tempdir().unwrap();
    let backup = external.path().join("backup");
    operations::backup(&store.config, &backup).unwrap();
    let restored = external.path().join("restored");
    operations::restore(&backup, &restored).unwrap();
    let mut config = store.config.clone();
    config.root = restored;
    let reopened = Store::initialize(config).unwrap();
    let provider = reopened.collector(&id, paycom::PROVIDER).unwrap();
    provider
        .one("SELECT count(*) FROM timecard_sources", [])
        .unwrap();
    assert_eq!(snapshot(&provider), before);
    assert_eq!(reopened.get_dsp(&id).unwrap()["status"], "suspended");
}
#[test]
fn cortex_storage_opens_without_the_emptied_delivery_history_tables() {
    let (_root, store, id) = provisioned();
    let cortex = store.collector(&id, cortex::PROVIDER).unwrap();
    cortex
        .0
        .execute_batch(
            "DROP TRIGGER minimize_legacy_meal_publication; DROP TABLE \
        meal_breaks; DROP TABLE meal_delivery_events;",
        )
        .unwrap();
    drop(cortex);
    store.open_collectors(&id).unwrap();
    let config = store.config.clone();
    drop(store);
    let reopened = Store::initialize(config).unwrap();
    let cortex = reopened.collector(&id, cortex::PROVIDER).unwrap();
    assert!(cortex.one("SELECT count(*) FROM meal_breaks", []).is_err());
    cortex.one("SELECT count(*) FROM meal_records", []).unwrap();
    // The tables that hold meal evidence are still required.
    cortex.0.execute_batch("DROP TABLE meal_records").unwrap();
    drop(cortex);
    assert!(reopened.open_collectors(&id).is_err());
}
