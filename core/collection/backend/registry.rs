//! Data providers. Each is described once, by a `Collector` in its own module, and
//! reached through `Provider`. Provider identities and paths are compiled code,
//! never user-controlled paths.
use crate::{
    Result,
    db::{self, Db, DspLease, Kind, Store, s},
    ensure,
    manifest::{Collector, Permission, perm, registry},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The DSP setting recording that its collectors' storage is kept apart.
pub const LAYOUT: &str = "storage.collectors";

/// What a member needs to manage the DSP's own accounts on Settings' Connections tab. It
/// exists while any connection does, or a page that adds one, and the role sheet lists it
/// under DSP Settings, after Manage DSP Settings.
pub const CONNECTIONS: Permission = perm("connections.manage", "Manage DSP Connections", 91)
    .group("DSP Settings")
    .recently_verified();

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
    pub fn collector(self) -> &'static dyn Collector {
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
            .ok_or_else(|| crate::Error::new("not_found", 404))
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
            .ok_or_else(|| crate::Error::new("unsupported_collector", 409))
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
pub fn added_identity(db: &Db, id: &str, provider: Provider, storage: &AddedStorage) -> Result<()> {
    identity(db, id, provider)?;
    let rows = db.all("SELECT source FROM storage_identity", [])?;
    ensure(
        s(&rows[0], "source") == storage.source,
        "collector_storage_identity_mismatch",
        503,
    )
}

/// The row a new database of a DSP's is created with, naming the DSP, the provider and
/// the storage it holds, so that it cannot pass for another's. Ids are compiled code or
/// validated, so they are safe SQL literals.
pub fn identity_seed(dsp: &str, provider: Provider, source: &str) -> String {
    format!(
        "INSERT INTO storage_identity VALUES ('{dsp}','{}','{source}');",
        provider.id()
    )
}
/// The connection row a collector's new database is created with, not yet connected.
pub fn connection_seed(provider: Provider) -> String {
    format!(
        "INSERT INTO connections(provider,updated_at) VALUES ('{}','{}');",
        provider.id(),
        db::iso()
    )
}
/// Adds the provider's connection row, not yet connected, unless it has one.
pub fn add_connection(db: &Db, provider: Provider) -> Result<()> {
    db.exec(
        "INSERT OR IGNORE INTO connections(provider,updated_at) VALUES (?,?)",
        [provider.id(), &db::iso()],
    )?;
    Ok(())
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
    pub fn clear_collector_browser_state(&self, id: &str, provider: Provider) -> Result<()> {
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
        crate::manifest::retirement::dsp(self, id)?;
        self.initialize_added_storages(id)?;
        self.reset_live(id)
    }
    /// A provider's added database, verified as that DSP's.
    pub fn added_storage(
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
                let target = Db::create(
                    &data.join(storage.id).join(format!("{}.sqlite", storage.id)),
                    storage.kind,
                    &identity_seed(id, provider, storage.source),
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
    pub fn open_collectors(&self, id: &str) -> Result<()> {
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
        crate::manifest::retirement::dsp(self, id)?;
        self.initialize_added_storages(id)?;
        self.reset_live(id)?;
        for provider in Provider::all() {
            provider.collector().opened(self, id)?;
        }
        Ok(())
    }

    fn reset_live(&self, id: &str) -> Result<()> {
        for provider in Provider::all() {
            crate::collection::live::reset(&*self.collector(id, provider)?)?;
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
            return self.verify_collector(id, provider);
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
        self.verify_collector(id, provider)
    }
    // The collector's own checks, then what each of its collections' keepers holds there.
    fn verify_collector(&self, id: &str, provider: Provider) -> Result<()> {
        provider
            .collector()
            .verify(&*self.collector(id, provider)?)?;
        for kind in provider.job_kinds() {
            registry()
                .keeper(kind)
                .verify(&*self.collector(id, provider)?)?;
        }
        Ok(())
    }
}
