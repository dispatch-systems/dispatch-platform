//! Data providers. Each is described once, by a `Collector` in its own module, and
//! reached through `Provider`. Provider identities and paths are compiled code,
//! never user-controlled paths.
#[path = "../../../collectors/cortex/collector.rs"]
pub mod cortex;
#[path = "../../../collectors/paycom/collector.rs"]
pub mod paycom;
use super::{
    Result,
    db::{self, Db, DspLease, Kind, Store, s},
    ensure,
    manifest::{Collector, Permission, perm, registry},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const LAYOUT: &str = "storage.collectors";

/// What a member needs to manage the DSP's connections. It exists while any connection
/// does, and the role sheet lists it under Connections.
pub const CONNECTIONS: Permission =
    perm("connections.manage", "Manage Connections", 70).group("Connections");

/// A provider, named by its collector's id. Each collector declares its own
/// (`PROVIDER`); what a provider is lives in its `Collector`, found in the registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Provider(&'static str);
/// A database the keeper of one of a provider's collections added beside the
/// provider's own: its files are named after `id`, and `marker` records that a DSP
/// gained it.
pub struct AddedStorage {
    pub id: &'static str,
    pub kind: Kind,
    pub marker: &'static str,
    /// Written into `storage_identity`, so a file cannot pass for another storage.
    pub source: &'static str,
    /// Fails closed when initialized storage lost what it must hold.
    pub verify: fn(&Db) -> Result<()>,
}
impl Provider {
    /// The provider whose collector's id is `id`. Only that collector names it.
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
    /// Every registered provider, in the registry's order.
    pub fn all() -> impl Iterator<Item = Self> {
        registry()
            .collectors
            .iter()
            .map(|collector| Self(collector.id()))
    }
    /// The registry: the one place that finds what a provider is.
    pub(crate) fn collector(self) -> &'static dyn Collector {
        registry()
            .collectors
            .iter()
            .copied()
            .find(|collector| collector.id() == self.0)
            .unwrap_or_else(|| panic!("the {} collector is not registered", self.0))
    }
    /// The provider every DSP was created with: the one whose storage was never added
    /// later. Addresses and fields from before there were others still name it.
    pub fn original() -> Option<Self> {
        Self::all().find(|provider| provider.marker().is_none())
    }
    pub fn parse(value: &str) -> Result<Self> {
        Self::all()
            .find(|p| p.id() == value)
            .ok_or_else(|| super::Error::new("not_found", 404))
    }
    pub fn key(self, dsp: &str) -> String {
        format!("{dsp}:{}", self.id())
    }
    pub fn id(self) -> &'static str {
        self.0
    }
    /// The kind of job that runs its main collection.
    pub fn job_kind(self) -> &'static str {
        self.collector().collections()[0].job_kind
    }
    /// Every kind of job this provider runs.
    pub fn job_kinds(self) -> impl Iterator<Item = &'static str> {
        self.collector()
            .collections()
            .iter()
            .map(|collection| collection.job_kind)
    }
    /// The databases its collections' keepers added beside its own, in the order of
    /// its collections.
    pub fn added_storages(self) -> impl Iterator<Item = &'static AddedStorage> {
        self.job_kinds()
            .flat_map(|kind| registry().keeper(kind).storages().iter().copied())
    }
    /// The provider of a job kind, and the kind as it is spelled in the registry.
    pub fn from_job_kind(kind: &str) -> Result<(Self, &'static str)> {
        Self::all()
            .find_map(|p| p.job_kinds().find(|k| *k == kind).map(|k| (p, k)))
            .ok_or_else(|| super::Error::new("unsupported_collector", 409))
    }
    pub fn validate_credentials(self, value: &Value) -> Result<()> {
        self.collector().validate_credentials(value)
    }
    fn database(self) -> Kind {
        self.collector().database()
    }
    fn marker(self) -> Option<&'static str> {
        self.collector().marker()
    }
    fn relative_path(self) -> PathBuf {
        Path::new(self.id()).join(format!("{}.sqlite", self.id()))
    }
}

fn identity(db: &Db, id: &str, provider: Provider) -> Result<()> {
    let rows = db.all("SELECT dsp_id,provider,source FROM storage_identity", [])?;
    ensure(
        rows.len() == 1 && s(&rows[0], "dsp_id") == id && s(&rows[0], "provider") == provider.id(),
        "collector_storage_identity_mismatch",
        503,
    )
}
pub(crate) fn added_identity(
    db: &Db,
    id: &str,
    provider: Provider,
    storage: &AddedStorage,
) -> Result<()> {
    identity(db, id, provider)?;
    let rows = db.all("SELECT source FROM storage_identity", [])?;
    ensure(
        s(&rows[0], "source") == storage.source,
        "collector_storage_identity_mismatch",
        503,
    )
}

/// Operator/benchmark read-only path resolution.
pub fn database_path(dsp_root: &Path, provider: Provider) -> Result<PathBuf> {
    let id = dsp_root.file_name().and_then(|s| s.to_str()).unwrap_or("");
    ensure(db::identifier(id, "dsp_"), "invalid_dsp_id", 400)?;
    let path = dsp_root.join("data").join(provider.relative_path());
    db::private_file(&path, false)?;
    Ok(path)
}

impl Store {
    /// Credential changes reset only this collector's sessions. Call after its
    /// browser worker closes; other collectors' profiles must survive unchanged.
    pub(crate) fn clear_collector_browser_state(&self, id: &str, provider: Provider) -> Result<()> {
        let browsers = self.area(id, "state")?.join("browsers");
        if !browsers.try_exists()? {
            return Ok(());
        }
        db::private_dir(&browsers)?;
        for entry in provider.collector().browser_entries() {
            let path = browsers.join(entry);
            match std::fs::symlink_metadata(&path) {
                Ok(stat) if stat.is_dir() => {
                    db::private_dir(&path)?;
                    std::fs::remove_dir_all(path)?;
                }
                Ok(_) => {
                    db::private_file(&path, false)?;
                    std::fs::remove_file(path)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn collector(&self, id: &str, provider: Provider) -> Result<DspLease<'_>> {
        let path = self.area(id, "data")?.join(provider.relative_path());
        let db = self.cached_database(&path, provider.database())?;
        identity(&db, id, provider)?;
        Ok(db)
    }

    pub(crate) fn initialize_collectors(&self, id: &str) -> Result<()> {
        for provider in Provider::all().filter(|p| p.marker().is_none()) {
            self.create_collector(id, provider)?;
        }
        self.dsp(id)?.set(LAYOUT, &json!(1))?;
        for provider in Provider::all().filter(|p| p.marker().is_some()) {
            self.initialize_added(id, provider)?;
        }
        self.initialize_added_storages(id)?;
        self.reset_live(id)
    }
    /// A provider's added database, verified as that DSP's.
    pub(crate) fn added_storage(
        &self,
        id: &str,
        provider: Provider,
        storage: &AddedStorage,
    ) -> Result<DspLease<'_>> {
        let path = self
            .area(id, "data")?
            .join(storage.id)
            .join(format!("{}.sqlite", storage.id));
        let db = self.cached_database(&path, storage.kind)?;
        added_identity(&db, id, provider, storage)?;
        Ok(db)
    }
    // Added databases follow the same path as added providers: created for a DSP
    // that lacks the marker, migrated and verified for one that has it.
    fn initialize_added_storages(&self, id: &str) -> Result<()> {
        for provider in Provider::all() {
            for storage in provider.added_storages() {
                let core = self.dsp(id)?;
                let marker = core.setting(storage.marker, Value::Null)?;
                if marker == json!(1) {
                    db::migrate(&*self.added_storage(id, provider, storage)?, storage.kind)?;
                    (storage.verify)(&*self.added_storage(id, provider, storage)?)?;
                    continue;
                }
                ensure(marker.is_null(), "unsupported_storage_layout", 503)?;
                let data = self.area(id, "data")?;
                db::private_dir(&data.join(storage.id))?;
                let seed = format!(
                    "INSERT INTO storage_identity VALUES ('{id}','{}','{}');",
                    provider.id(),
                    storage.source
                );
                let target = Db::create(
                    &data.join(storage.id).join(format!("{}.sqlite", storage.id)),
                    storage.kind,
                    &seed,
                )?;
                added_identity(&target, id, provider, storage)?;
                drop(target);
                core.set(storage.marker, &json!(1))?;
                (storage.verify)(&*self.added_storage(id, provider, storage)?)?;
            }
        }
        Ok(())
    }
    // Called during startup under the platform lock, before serving requests.
    // Provider databases are verified, and gain the migrations they lack.
    pub(crate) fn open_collectors(&self, id: &str) -> Result<()> {
        ensure(
            self.dsp(id)?.setting(LAYOUT, Value::Null)? == json!(1),
            "unsupported_storage_layout",
            503,
        )?;
        for provider in Provider::all() {
            if provider.marker().is_some() {
                self.initialize_added(id, provider)?;
            } else {
                db::migrate(&*self.collector(id, provider)?, provider.database())?;
            }
        }
        self.initialize_added_storages(id)?;
        self.reset_live(id)?;
        for provider in Provider::all() {
            provider.collector().opened(self, id)?;
        }
        Ok(())
    }

    fn reset_live(&self, id: &str) -> Result<()> {
        for provider in Provider::all() {
            super::live_collection::reset(&*self.collector(id, provider)?)?;
        }
        Ok(())
    }

    // Validated random DSP IDs and compiled provider IDs are safe SQL literals.
    fn create_collector(&self, id: &str, provider: Provider) -> Result<Db> {
        let data = self.area(id, "data")?;
        db::private_dir(&data.join(provider.id()))?;
        let target = Db::create(
            &data.join(provider.relative_path()),
            provider.database(),
            &provider.collector().seed(id),
        )?;
        identity(&target, id, provider)?;
        Ok(target)
    }

    // New provider storage is additive and initialized before serving traffic.
    // A core marker distinguishes first installation from missing/lost state.
    fn initialize_added(&self, id: &str, provider: Provider) -> Result<()> {
        let collector = provider.collector();
        let key = collector.marker().expect("added collector");
        let core = self.dsp(id)?;
        let marker = core.setting(key, Value::Null)?;
        if marker == json!(1) {
            db::migrate(&*self.collector(id, provider)?, provider.database())?;
            return collector.verify(&*self.collector(id, provider)?);
        }
        ensure(marker.is_null(), "unsupported_storage_layout", 503)?;
        let target = self.create_collector(id, provider)?;
        ensure(
            target
                .one(
                    "SELECT provider FROM connections WHERE provider=?",
                    [provider.id()],
                )?
                .is_some(),
            "collector_storage_invalid",
            503,
        )?;
        core.set(key, &json!(1))?;
        collector.verify(&*self.collector(id, provider)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, operations, workforce};
    use paycom::fixtures;
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
            .publish(&id, &fixtures::fixture("UTC").unwrap())
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
        reopened.publish(&id, &next).unwrap();
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
        let provider = Db::open(&path, Kind::Paycom).unwrap();
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
            crate::contracts::ConnectionStatus::Ready
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
    #[test]
    fn missing_initialized_cortex_database_is_not_recreated() {
        let (_root, store, id) = provisioned();
        let path =
            database_path(&store.config.root.join("dsps").join(&id), cortex::PROVIDER).unwrap();
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
}
