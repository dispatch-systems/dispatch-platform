//! The collector registry and the storage it opens for each collector, with Paycom and
//! Cortex registered and Timecard's data in their databases.
use crate::{
    testing,
    workforce::{self, TimecardStore},
};
use dispatch_core::{
    collection::registry::{LAYOUT, Provider, database_path},
    db::{self, Db, Store},
    foundation::config::Config,
    server::operations,
};
use dispatch_cortex as cortex;
use dispatch_paycom as paycom;
use paycom::fixtures;
use serde_json::{Value, json};
use std::os::unix::fs::{PermissionsExt, symlink};

fn platform() -> (tempfile::TempDir, Store) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = Config::load().unwrap();
    config.root = root.path().into();
    let store = Store::initialize(config).unwrap();
    (root, store)
}
fn pending(store: &Store) -> String {
    let id = dispatch_core::foundation::crypto::id("dsp").unwrap();
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
fn the_registry_names_each_provider_job_kind_database_and_schedule_once() {
    let unique = |values: Vec<&str>| {
        values
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == values.len()
    };
    let all = || Provider::all().map(|p| p.collector());
    assert!(unique(all().map(|c| c.id()).collect()));
    assert!(unique(
        Provider::all().flat_map(|p| p.job_kinds()).collect()
    ));
    assert!(unique(all().filter_map(|c| c.marker()).collect()));
    assert!(unique(
        all()
            .flat_map(|c| c.collections().iter().map(|s| s.schedule))
            .collect()
    ));
    assert!(unique(
        Provider::all()
            .flat_map(|p| p.added_storages().map(|s| s.marker))
            .collect()
    ));
    for provider in Provider::all() {
        let collector = provider.collector();
        assert_eq!(Provider::parse(collector.id()).unwrap(), provider);
        for kind in provider.job_kinds() {
            assert_eq!(Provider::from_job_kind(kind).unwrap(), (provider, kind));
        }
        for storage in provider.added_storages() {
            assert_eq!(storage.kind.name(), storage.id);
            assert!(storage.marker.starts_with("storage."));
        }
        // Storage, secrets and profiles are all named after the id.
        assert_eq!(collector.database().name(), collector.id());
        assert!(collector.seed("dsp_test").contains(collector.id()));
        assert!(
            collector
                .browser_entries()
                .contains(&format!("{}-attempt.json", collector.id()).as_str())
        );
    }
}
#[test]
fn startup_refuses_a_dsp_whose_provider_storage_was_never_separated() {
    let (_root, store, id) = provisioned();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    store
        .dsp(&id)
        .unwrap()
        .exec("DELETE FROM settings WHERE key=?", [LAYOUT])
        .unwrap();
    let config = store.config.clone();
    drop(store);
    assert_eq!(
        Store::initialize(config.clone()).err().unwrap().code,
        "unsupported_storage_layout"
    );
    let path = database_path(&config.root.join("dsps").join(&id), paycom::PROVIDER).unwrap();
    let provider = Db::open(&path, paycom::DATABASE).unwrap();
    assert_eq!(snapshot(&provider), before);
}
#[test]
fn storage_survives_backup_and_restore() {
    let (_root, store, id) = provisioned();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    let pulse = db::private_dir(
        &store
            .area(&id, "state")
            .unwrap()
            .join("browsers/paycom-browseros/config/pulse"),
    )
    .unwrap();
    symlink(
        "/tmp/obsolete-browser-runtime",
        pulse.join("dispatch-server-runtime"),
    )
    .unwrap();
    let external = tempfile::tempdir().unwrap();
    let backup = external.path().join("backup");
    operations::backup(&store.config, &backup).unwrap();
    let restored = external.path().join("restored");
    operations::restore(&backup, &restored).unwrap();
    assert!(
        restored
            .join("dsps")
            .join(&id)
            .join("state/browsers/paycom-browseros/config/pulse/dispatch-server-runtime")
            .symlink_metadata()
            .is_err()
    );
    // Unknown symlinks still fail closed; only known browser runtime links skip.
    symlink("/tmp/not-profile-data", pulse.join("unexpected-link")).unwrap();
    assert!(operations::backup(&store.config, &external.path().join("unsafe-backup")).is_err());
    let mut config = store.config.clone();
    config.root = restored;
    let reopened = Store::initialize(config).unwrap();
    reopened.open_collectors(&id).unwrap();
    assert_eq!(
        snapshot(&reopened.collector(&id, paycom::PROVIDER).unwrap()),
        before
    );
}
#[test]
fn cortex_storage_recovers_initialization_and_preserves_provider_identity() {
    let (_root, store, id) = provisioned();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    store
        .collector(&id, cortex::PROVIDER)
        .unwrap()
        .exec(
            "UPDATE connections SET enabled=1,status='ready',revision=8",
            [],
        )
        .unwrap();
    // A crash after creating the database but before writing the core marker.
    store
        .dsp(&id)
        .unwrap()
        .exec("DELETE FROM settings WHERE key='storage.cortex'", [])
        .unwrap();
    store.open_collectors(&id).unwrap();
    assert_eq!(
        store.connection_for(&id, cortex::PROVIDER).unwrap().status,
        dispatch_core::collection::api::types::ConnectionStatus::Ready
    );
    assert_eq!(
        snapshot(&store.collector(&id, paycom::PROVIDER).unwrap()),
        before
    );
    assert!(Provider::from_job_kind("cortex.collect").is_err());
    store
        .collector(&id, cortex::PROVIDER)
        .unwrap()
        .exec("UPDATE storage_identity SET dsp_id='another-dsp'", [])
        .unwrap();
    assert!(store.open_collectors(&id).is_err());
}
#[test]
fn missing_initialized_cortex_database_is_not_recreated() {
    let (_root, store, id) = provisioned();
    let path = database_path(&store.config.root.join("dsps").join(&id), cortex::PROVIDER).unwrap();
    let config = store.config.clone();
    drop(store);
    std::fs::remove_file(&path).unwrap();
    assert!(Store::initialize(config).is_err());
    assert!(!path.exists());
}
#[test]
fn resetting_paycom_browser_state_preserves_other_collectors_and_business_data() {
    let (_root, store, id) = provisioned();
    let before = snapshot(&store.collector(&id, paycom::PROVIDER).unwrap());
    let browsers = store.area(&id, "state").unwrap().join("browsers");
    for name in ["paycom", "paycom-browseros", "future-collector"] {
        db::private_dir(&browsers.join(name)).unwrap();
        db::write_private(&browsers.join(name).join("session"), b"private session").unwrap();
    }
    db::write_private(&browsers.join("paycom-attempt.json"), b"{}").unwrap();
    store
        .clear_collector_browser_state(&id, paycom::PROVIDER)
        .unwrap();
    assert!(!browsers.join("paycom").exists());
    assert!(!browsers.join("paycom-browseros").exists());
    assert!(!browsers.join("paycom-attempt.json").exists());
    assert_eq!(
        std::fs::read(browsers.join("future-collector/session")).unwrap(),
        b"private session"
    );
    assert_eq!(
        snapshot(&store.collector(&id, paycom::PROVIDER).unwrap()),
        before
    );
    store
        .clear_collector_browser_state(&id, paycom::PROVIDER)
        .unwrap();
}
#[test]
fn refuses_missing_cross_tenant_and_unsafe_storage() {
    let (root, store) = platform();
    let id = pending(&store);
    let data = store.area(&id, "data").unwrap();
    let outside = root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    symlink(&outside, data.join("paycom")).unwrap();
    assert!(store.provision(&id).is_err());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    std::fs::remove_file(data.join("paycom")).unwrap();
    store.provision(&id).unwrap();
    store
        .collector(&id, paycom::PROVIDER)
        .unwrap()
        .exec("UPDATE storage_identity SET dsp_id='another-tenant'", [])
        .unwrap();
    assert_eq!(
        store.collector(&id, paycom::PROVIDER).err().unwrap().code,
        "collector_storage_identity_mismatch"
    );
    let config = store.config.clone();
    let key = store.key.clone();
    drop(store);
    std::fs::remove_file(data.join("paycom/paycom.sqlite")).unwrap();
    let store = Store::open(config, key).unwrap();
    assert!(store.collector(&id, paycom::PROVIDER).is_err());
    assert!(!data.join("paycom/paycom.sqlite").exists());
    assert!(Provider::from_job_kind("../../unknown.collect").is_err());
    assert!(store.collector("../../escape", paycom::PROVIDER).is_err());
}
#[test]
fn startup_rejects_provider_database_as_core_storage() {
    let (_root, db) = testing::seeded();
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
