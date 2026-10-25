use super::{Db, Kind, identifier, key_file, migrations, private_dir, private_file, s};
use crate::{Result, ensure, foundation::config::Config};
use std::path::{Path, PathBuf};

/// The DSP folder an owner keeps files in that it can collect again, which backups leave out.
pub const CACHE: &str = "cache";
/// A connection to one of a DSP's databases. Every lease of the same file within one store's
/// work shares its connection, as the platform's database is shared, so work that reaches a
/// DSP's database twice sees, and joins, the same transaction.
pub struct DspLease<'a> {
    db: std::rc::Rc<Db>,
    _store: std::marker::PhantomData<&'a Store>,
}
impl std::ops::Deref for DspLease<'_> {
    type Target = Db;
    fn deref(&self) -> &Db {
        &self.db
    }
}

/// The DSP databases a store has open, each shared by every lease of it.
struct DspCache(std::cell::RefCell<Vec<(String, std::rc::Rc<Db>)>>);
// SAFETY: the counted connections are reached only through the store, which is not `Sync`,
// and through leases that borrow it and are not `Send`. A store moves to another thread only
// once no lease borrows it, so every count moves with it and none is ever touched from two
// threads.
unsafe impl Send for DspCache {}

pub struct Store {
    pub config: Config,
    pub platform: Db,
    pub jobs: Db,
    pub key: Vec<u8>,
    dsp_cache: DspCache,
    // Set when this work queued mail; the state wakes the mailer once the work is done.
    mail_queued: std::cell::Cell<bool>,
    // Set when this work moved when schedules next run; the state wakes the scheduler once
    // the work is done.
    schedules_moved: std::cell::Cell<bool>,
    // Set when this work switched a feature or saved settings; the state wakes the live
    // readers once the work is done.
    readers_moved: std::cell::Cell<bool>,
}
impl Store {
    pub fn initialize(config: Config) -> Result<Self> {
        private_dir(&config.root)?;
        private_dir(&config.root.join("config"))?;
        private_dir(&config.root.join("data"))?;
        private_dir(&config.platform())?;
        private_dir(&config.environment_root())?;
        private_dir(&config.root.join("dsps"))?;
        let key = key_file(&config.platform().join("platform.key"))?;
        let store = Self {
            platform: Db::create(
                &config.platform().join("accounts.sqlite"),
                Kind::PLATFORM,
                "",
            )?,
            jobs: Db::create(
                &config.environment_root().join("jobs.sqlite"),
                Kind::JOBS,
                "",
            )?,
            config,
            key,
            dsp_cache: DspCache(std::cell::RefCell::new(Vec::new())),
            mail_queued: std::cell::Cell::new(false),
            schedules_moved: std::cell::Cell::new(false),
            readers_moved: std::cell::Cell::new(false),
        };
        crate::manifest::retirement::platform(&store)?;
        store.keep_mandatory_features()?;
        crate::tenancy::roles::backfill(&store.platform)?;
        for row in store.platform.all(
            "SELECT id FROM dsps WHERE status IN ('active','suspended')",
            [],
        )? {
            let id = s(&row, "id");
            migrations::migrate_dsp(&*store.dsp_unverified(id)?, id)?;
            store.open_collectors(id)?;
            store.initialize_schedules(id)?;
        }
        // Each DSP's people move into its own directory once, then its roles are repaired as
        // the platform's were: a role for each legacy membership, and no retired permission.
        for id in store.kept_dsps()? {
            store.move_people(&id)?;
            let people = store.dsp(&id)?;
            crate::tenancy::roles::backfill_directory(&people, &id)?;
            crate::manifest::retirement::roles(&people)?;
        }
        store.backfill_codes()?;
        Ok(store)
    }
    pub fn open(config: Config, key: Vec<u8>) -> Result<Self> {
        Ok(Self {
            platform: Db::open(&config.platform().join("accounts.sqlite"), Kind::PLATFORM)?,
            jobs: Db::open(&config.environment_root().join("jobs.sqlite"), Kind::JOBS)?,
            config,
            key,
            dsp_cache: DspCache(std::cell::RefCell::new(Vec::new())),
            mail_queued: std::cell::Cell::new(false),
            schedules_moved: std::cell::Cell::new(false),
            readers_moved: std::cell::Cell::new(false),
        })
    }
    /// Records that this work queued mail for delivery.
    pub fn mail_queued(&self) {
        self.mail_queued.set(true);
    }
    /// Whether the work since the last call queued mail.
    pub fn take_mail_queued(&self) -> bool {
        self.mail_queued.replace(false)
    }
    /// Records that this work moved when a DSP's schedules next run, as retiming them does.
    pub fn schedules_moved(&self) {
        self.schedules_moved.set(true);
    }
    /// Whether the work since the last call moved when schedules next run.
    pub fn take_schedules_moved(&self) -> bool {
        self.schedules_moved.replace(false)
    }
    /// Records that this work may have moved when a DSP's live readers read: it switched a
    /// feature, or saved settings a reader's window comes from.
    pub fn readers_moved(&self) {
        self.readers_moved.set(true);
    }
    /// Whether the work since the last call may have moved when live readers read.
    pub fn take_readers_moved(&self) -> bool {
        self.readers_moved.replace(false)
    }
    /// One of a DSP's folders: `data`, `config`, `state`, `secrets`, or `cache`, where an
    /// owner keeps files it can collect again. Backups leave `cache` out, and it goes with
    /// the DSP's folder.
    pub fn area(&self, dsp: &str, area: &str) -> Result<PathBuf> {
        ensure(identifier(dsp, "dsp_"), "invalid_dsp_id", 400)?;
        ensure(
            ["data", "config", "state", "secrets", CACHE].contains(&area),
            "invalid_area",
            400,
        )?;
        let root = self.config.root.join("dsps").join(dsp);
        private_dir(&root)?;
        private_dir(&root.join(area))
    }
    pub fn dsp(&self, id: &str) -> Result<DspLease<'_>> {
        let db = self.dsp_unverified(id)?;
        migrations::verify_dsp_identity(&db, id)?;
        Ok(db)
    }
    fn dsp_unverified(&self, id: &str) -> Result<DspLease<'_>> {
        let path = self.area(id, "data")?.join("dispatch.sqlite");
        self.cached_database(&path, Kind::DSP)
    }
    pub(crate) fn cached_database(&self, path: &Path, kind: Kind) -> Result<DspLease<'_>> {
        private_file(path, false)?;
        ensure(path.is_file(), "storage_file_missing", 503)?;
        let id = path.to_string_lossy();
        let mut cache = self.dsp_cache.0.borrow_mut();
        // The most recently used stay, four at most; one still leased outlives its eviction.
        let db = match cache.iter().position(|(key, _)| key == &id) {
            Some(index) => cache.remove(index).1,
            None => std::rc::Rc::new(Db::open(path, kind)?),
        };
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((id.into_owned(), db.clone()));
        Ok(DspLease {
            db,
            _store: std::marker::PhantomData,
        })
    }
    pub fn initialize_dsp(&self, id: &str) -> Result<Db> {
        for area in ["data", "config", "state", "secrets"] {
            self.area(id, area)?;
        }
        Db::create_dsp(&self.area(id, "data")?.join("dispatch.sqlite"), id)
    }
}
