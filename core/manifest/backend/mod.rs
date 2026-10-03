//! What collectors and features declare, and the one registry the app builds from them.
//! Core reaches collectors and features through `registry()`, never by name.
use crate::{
    Code, Error, Result, State,
    browsers::{Collected, Driver, Pending, browseros},
    collectors::AddedStorage,
    db::{Db, Kind, Store},
    job_metrics::Counts,
};
use serde_json::Value;
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

/// Everything this build is made of. The app builds one and installs it at startup.
pub struct Registry {
    pub collectors: &'static [&'static dyn Collector],
    pub features: &'static [&'static Feature],
}

static INSTALLED: OnceLock<&'static Registry> = OnceLock::new();

/// Makes `registry` the one `registry()` answers. Installing it again changes nothing;
/// installing a different one panics.
pub fn install(registry: &'static Registry) {
    let installed = *INSTALLED.get_or_init(|| registry);
    assert!(
        std::ptr::eq(installed, registry),
        "a different registry is already installed"
    );
}

/// The installed registry.
pub fn registry() -> &'static Registry {
    // A module's own tests run without the app's startup, so they find the app's registry.
    #[cfg(test)]
    INSTALLED.get_or_init(|| &crate::REGISTRY);
    INSTALLED
        .get()
        .copied()
        .expect("no registry is installed: the app installs one before anything reads it")
}

/// What a feature declares in its `feature.rs`. Fields join as the slots that read them land.
pub struct Feature {
    /// Its directory's name.
    pub name: &'static str,
}
/// A feature that fills no slot yet. A manifest starts here and names what it adds:
/// `Feature { …, ..feature("timecard") }`.
pub const fn feature(name: &'static str) -> Feature {
    Feature { name }
}

/// Everything the platform needs to know about one provider. Storage, credentials,
/// the browser and the job queue ask here instead of matching on the provider.
pub trait Collector: Sync {
    /// Names its connection row, its files, its secrets and its API path.
    fn id(&self) -> &'static str;
    /// Its name as the dashboard shows it.
    fn label(&self) -> &'static str;
    /// What its connection supplies to the pages that require it (`features`).
    fn capabilities(&self) -> &'static [&'static str];
    /// The kind of job that runs its main collection.
    fn job_kind(&self) -> &'static str;
    /// The kinds of its other collections, each chosen by `job_kind_for`.
    fn other_job_kinds(&self) -> &'static [&'static str] {
        &[]
    }
    /// The kind of job a request queues.
    fn job_kind_for(&self, _request: &Value) -> &'static str {
        self.job_kind()
    }
    /// Adds current tenant context needed only while a queued request executes. The
    /// persisted request stays compatible with the previous binary for rollback.
    fn bind_request(&self, _: &Store, _: &str, request: &Value) -> Result<Value> {
        Ok(request.clone())
    }
    /// Databases beside its own, one per added collection.
    fn added_storages(&self) -> &'static [&'static AddedStorage] {
        &[]
    }
    /// Its database, and with it the migration list in `db::schema`.
    fn database(&self) -> Kind;
    /// Written into a new database with its schema. Must identify the storage.
    fn seed(&self, dsp: &str) -> String;
    /// The DSP setting recording that this storage was added to an existing DSP.
    /// `None` only for Paycom, whose storage every DSP was created with.
    fn marker(&self) -> Option<&'static str>;
    /// Fails closed when initialized storage lost what it must hold.
    fn verify(&self, _: &Db) -> Result<()> {
        Ok(())
    }
    /// After every collector of a DSP opened at startup.
    fn opened(&self, _: &Store, _dsp: &str) -> Result<()> {
        Ok(())
    }
    /// What a credential change removes from `state/browsers`.
    fn browser_entries(&self) -> &'static [&'static str];
    /// The hosts its browser may reach, as its own `HostPolicy`.
    fn network(&self) -> browseros::NetworkPolicy;
    fn validate_credentials(&self, value: &Value) -> Result<()>;
    /// Shown beside the connection. Never a secret.
    fn account_label<'a>(&self, _credentials: &'a Value) -> &'a str {
        ""
    }
    /// Its driver, on a browser already running under `network`. `fixture` is the
    /// origin of a local stand-in for the provider's site.
    fn driver<'a>(
        &self,
        browser: browseros::Session,
        profile: &'a Path,
        fixture: Option<&'a str>,
    ) -> Pending<'a, Box<dyn Driver>>;
    /// What a collection returns in fixture mode, where no browser runs.
    fn fixture(&self, timezone: &str, request: &Value) -> Result<Collected>;
    /// The job's message while it collects `request`.
    fn progress(&self, _request: &Value) -> &'static str;
    /// What a finished collection brought, as the job's metrics count it.
    fn counts(&self, _data: &Value) -> Counts {
        Counts::default()
    }
    /// Whether its collections bring the whole roster, so that a finished one naming
    /// no single employee may have changed who is on it.
    fn roster(&self) -> bool {
        false
    }
    /// Every error code its collections raise and branch on.
    fn codes(&self) -> &'static [Code] {
        &[]
    }
    /// Those of its codes a failed attempt is queued again after, while attempts remain.
    fn retryable(&self) -> &'static [Code] {
        &[]
    }
    /// Work a finished collection needs before it is stored, done without the database
    /// and outside the platform lock: parsing, shaping rows, compressing.
    fn prepare(&self, collected: Collected) -> Result<Collected> {
        Ok(collected)
    }
    /// Stores what it can of a finished collection before the job publishes it, in
    /// steps short enough that the platform lock is never held for long; `publish` then
    /// only makes it current. Each step checks the job is still this worker's.
    fn stage<'a>(
        &'a self,
        _state: &'a Arc<State>,
        _dsp: &'a str,
        _job: &'a str,
        _owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move { Ok(collected) })
    }
    /// Stores a finished collection. Runs while the job is still this worker's.
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()>;
    /// Drops what an unfinished job kept to resume from. `None` means every job.
    fn discard(&self, _: &Store, _dsp: &str, _job: Option<&str>) -> Result<()> {
        Ok(())
    }
    /// When `date` was last collected, as a row with `collected_at`.
    fn collected_at(&self, db: &Db, date: &str) -> Result<Option<Value>>;
    /// The schedule `collection`s that run this collector, each with the error a
    /// schedule answers while it is not connected. `both` runs the first of every
    /// collector's.
    fn schedules(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
    /// What else a schedule needs before it can run `collection`.
    fn schedule_ready(&self, _: &Store, _dsp: &str, _collection: &str) -> Result<()> {
        Ok(())
    }
    /// The jobs one scheduled run of `collection` queues: an idempotency key suffix
    /// and a request each.
    fn scheduled(&self, _: &Store, _dsp: &str, _collection: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![])
    }
    /// Runs with the connection's own disable, in its transaction.
    fn disabled(&self, _: &Db) -> Result<()> {
        Ok(())
    }
}

/// Core's error codes, joined by the ones each registered collector declares.
impl Code {
    /// Every code the backend branches on: core's own, then each collector's.
    pub fn all() -> impl Iterator<Item = Code> {
        let collectors = registry().collectors.iter().flat_map(|c| c.codes());
        Code::ALL.iter().chain(collectors).copied()
    }
    /// A failed attempt with one of these is queued again while attempts remain: core's
    /// own, then each collector's.
    pub fn retryable() -> impl Iterator<Item = Code> {
        let collectors = registry().collectors.iter().flat_map(|c| c.retryable());
        Code::RETRYABLE.iter().chain(collectors).copied()
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::all().find(|code| code.as_str() == text)
    }
    pub fn is_retryable(self) -> bool {
        Self::retryable().any(|code| code == self)
    }
}
impl Error {
    pub fn known(&self) -> Option<Code> {
        Code::parse(&self.code)
    }
    pub fn is_retryable(&self) -> bool {
        Code::retryable().any(|code| self.is(code))
    }
}
