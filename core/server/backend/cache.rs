//! Bounded derived reads shared across database workers. Call only after authorization,
//! while holding State's transition read lock. Unknown writes advance the global
//! revision; a write on a DSP's route evicts that DSP's scopes and those every DSP shares;
//! reviewed writes evict the scopes that depend on their changed data.
use crate::{Error, Result, manifest::registry};
use serde::Serialize;
use std::any::Any;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

/// Identical expensive reads take turns before entering the database pool. The next
/// reader checks authorization and the cache again, so neither credentials nor errors
/// are shared. Keep the turn inside the database closure if its caller is cancelled.
pub struct ReadTurns {
    keys: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    slots: Arc<tokio::sync::Semaphore>,
}
pub struct ReadTurn {
    _slot: tokio::sync::OwnedSemaphorePermit,
    _lock: tokio::sync::OwnedMutexGuard<()>,
}
impl Default for ReadTurns {
    fn default() -> Self {
        Self {
            keys: Mutex::new(HashMap::new()),
            slots: Arc::new(tokio::sync::Semaphore::new(64)),
        }
    }
}
impl ReadTurns {
    pub async fn enter(&self, key: String) -> Result<ReadTurn> {
        self.enter_for(key, Duration::from_secs(15)).await
    }
    /// Takes a keyed turn with the caller's waiting budget, before entering the database pool.
    pub(crate) async fn enter_for(&self, key: String, wait: Duration) -> Result<ReadTurn> {
        let slot = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|error| match error {
                tokio::sync::TryAcquireError::NoPermits => Error::busy("read_turns_full"),
                tokio::sync::TryAcquireError::Closed => Error::busy("read_turns_closed"),
            })?;
        let lock = {
            let mut keys = self
                .keys
                .lock()
                .map_err(|_| Error::busy("read_turns_poisoned"))?;
            keys.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = keys.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                keys.insert(key, Arc::downgrade(&lock));
                lock
            }
        };
        let lock = tokio::time::timeout(wait, lock.lock_owned())
            .await
            .map_err(|_| Error::busy("read_turns_key_timeout"))?;
        Ok(ReadTurn {
            _slot: slot,
            _lock: lock,
        })
    }
}

struct Entry {
    key: String,
    scope: Scope,
    revision: u64,
    /// When it's no longer kept.
    until: Instant,
    value: Box<dyn Any + Send>,
    bytes: usize,
}
/// A cached read: what it reads, named by the id its dependencies are declared under
/// (`Cached`), and for which DSP. The DSP listings are every DSP's at once.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Scope {
    read: &'static str,
    dsp: Option<String>,
}
impl Scope {
    /// Every DSP's listing: memberships, features, profile, connections and collection
    /// dates.
    pub fn listings() -> Self {
        Self {
            read: LISTINGS,
            dsp: None,
        }
    }
    /// One DSP's `read`.
    pub fn tenant(read: &'static str, dsp: impl Into<String>) -> Self {
        Self {
            read,
            dsp: Some(dsp.into()),
        }
    }
}
/// The DSP listings' cached read.
pub const LISTINGS: &str = "listings";
/// A DSP's people: driver decisions/preferences and identities/activity from every
/// collection.
pub const PEOPLE: &str = "people";

/// A kind of data a reviewed write changes, named by the id its owner declares: core's
/// here, a feature's in its manifest. Use State::run for anything not covered by one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataDomain(&'static str);
impl DataDomain {
    /// Temporary pages and itineraries a collection shows while it runs.
    pub const LIVE: Self = Self("live");
    /// Profile, features, connections, permissions or other tenant-wide settings.
    pub const TENANT: Self = Self("tenant");
    /// Only scheduled collection dates, also shown in session DSP listings.
    pub const SCHEDULES: Self = Self("schedules");
    /// Who is who among a DSP's people: the decisions the feature answering the directory
    /// keeps, which any feature showing people reads.
    pub const DRIVERS: Self = Self("drivers");
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
    pub fn id(self) -> &'static str {
        self.0
    }
    /// What a finished collection of `kind` writes, as its keeper declares. A keeper that
    /// declares none keeps the conservative tenant-wide dependency set.
    pub fn collection(kind: crate::collection::api::jobs::JobKind) -> Self {
        registry().keeper(kind.as_str()).domain()
    }
}
/// Core's own domains, beside the features'.
pub const DOMAINS: &[DataDomain] = &[
    DataDomain::LIVE,
    DataDomain::TENANT,
    DataDomain::SCHEDULES,
    DataDomain::DRIVERS,
];

/// A reviewed preparation's proof about every input to every PEOPLE read, including
/// identity, activity, workdays, matching decisions, settings and source availability.
/// Uncertainty must remain `MayChange`; unchanged counts or directory JSON are insufficient.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PeopleImpact {
    #[default]
    MayChange,
    Unchanged,
}

/// Which writes evict a cached read, as an owner that knows of the dependency declares it.
/// A read is evicted by a write any of its declarations names.
pub struct Cached {
    pub read: &'static str,
    pub evicted: Evicted,
}
pub enum Evicted {
    /// By writes to these domains.
    By(&'static [DataDomain]),
    /// By writes to any domain but these.
    ByAllBut(&'static [DataDomain]),
    /// By writes to these domains, and to each domain a feature that names people declares or
    /// publishes a collection to: what who is who is read from.
    ByPeopleAnd(&'static [DataDomain]),
}
/// What core declares of its own cached reads.
pub const CACHED: &[Cached] = &[
    Cached {
        read: LISTINGS,
        evicted: Evicted::By(&[DataDomain::TENANT, DataDomain::SCHEDULES]),
    },
    // Driver Match reads what each feature naming people publishes, and its own decisions,
    // never a collection's live pages.
    Cached {
        read: PEOPLE,
        evicted: Evicted::ByPeopleAnd(&[DataDomain::TENANT, DataDomain::DRIVERS]),
    },
    // Settings reads its sections' values, and whatever their options and keepers read of the
    // DSP's data, never a collection's live pages.
    Cached {
        read: crate::settings::CACHED,
        evicted: Evicted::ByAllBut(&[DataDomain::LIVE]),
    },
];
fn evicts(read: &str, domain: DataDomain) -> bool {
    registry()
        .cached()
        .filter(|cached| cached.read == read)
        .any(|cached| match cached.evicted {
            Evicted::By(domains) => domains.contains(&domain),
            Evicted::ByAllBut(domains) => !domains.contains(&domain),
            Evicted::ByPeopleAnd(domains) => {
                domains.contains(&domain) || registry().naming_people(domain)
            }
        })
}
/// How long a read is kept at most, though no write evicts it. Most reads, which may depend on
/// the time, half a minute. Who is who doesn't, and each write that could change it evicts it:
/// ten minutes, so a write that changed it without saying so shows within them.
fn lifetime(read: &str) -> Duration {
    if read == PEOPLE {
        Duration::from_secs(600)
    } else {
        Duration::from_secs(30)
    }
}
/// The most the read cache keeps in all, by its estimates: room for the working sets of
/// several busy DSPs at once, each a people directory with its feature's reads beside it, so
/// one DSP's reads don't push another's out.
const KEPT: usize = 64 * 1024 * 1024;
/// The most one result may be, by its estimate, to be kept: no one result fills the cache.
const LARGEST: usize = 16 * 1024 * 1024;
/// The most results the read cache keeps.
const ENTRIES: usize = 256;
#[derive(Clone, PartialEq, Eq, Hash)]
struct SharedRead {
    scope: Scope,
    key: String,
    revision: u64,
}
#[derive(Default)]
struct SharedValue(Mutex<Option<Box<dyn Any + Send>>>);
impl SharedValue {
    fn read<T: Send + Sync + 'static>(
        &self,
        load: impl FnOnce() -> Result<Arc<T>>,
    ) -> Result<Arc<T>> {
        let mut shared = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = shared
            .as_ref()
            .and_then(|value| value.downcast_ref::<Arc<T>>())
        {
            return Ok(value.clone());
        }
        let value = load()?;
        // Joined readers retain this result even if the bounded cache rejects or evicts it.
        // Only their strong handles keep it alive; errors and panics leave nothing to share.
        *shared = Some(Box::new(value.clone()));
        Ok(value)
    }
}
#[derive(Default)]
pub struct ReadCache {
    entries: Mutex<VecDeque<Entry>>,
    /// Only live shared calculations retain a turn; a later miss can retry an error or panic.
    making: Mutex<HashMap<SharedRead, Weak<SharedValue>>>,
}
impl ReadCache {
    /// Keep one immutable derived result when pages need different slices of it.
    /// Cloning its Arc neither copies the roster nor repeats JSON parsing.
    pub fn shared<T: Serialize + Send + Sync + 'static>(
        &self,
        scope: Scope,
        key: String,
        revision: u64,
        load: impl FnOnce() -> Result<T>,
    ) -> Result<Arc<T>> {
        if let Some(value) = self.cached(&scope, &key, revision) {
            return Ok(value);
        }
        let making = self.begin_shared(SharedRead {
            scope: scope.clone(),
            key: key.clone(),
            revision,
        });
        making.read(|| {
            self.read_sized(
                scope,
                key,
                revision,
                || Ok(Arc::new(load()?)),
                |value| Ok(serde_json::to_vec(value.as_ref())?.len().saturating_mul(2)),
            )
        })
    }
    /// An admitted immutable result, without loading on a miss. Like shared(), call only
    /// after authorization under State's transition read lock, using data_revision.
    /// In-flight results rejected by the retention budget are deliberately not returned.
    pub fn lookup_shared<T: Send + Sync + 'static>(
        &self,
        scope: &Scope,
        key: &str,
        revision: u64,
    ) -> Option<Arc<T>> {
        self.cached(scope, key, revision)
    }
    /// Join before waiting: every joined reader holds the same in-flight value alive.
    fn begin_shared(&self, named: SharedRead) -> Arc<SharedValue> {
        let mut keys = self
            .making
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        keys.retain(|_, value| value.strong_count() > 0);
        match keys.get(&named).and_then(Weak::upgrade) {
            Some(value) => value,
            None => {
                let value = Arc::new(SharedValue::default());
                keys.insert(named, Arc::downgrade(&value));
                value
            }
        }
    }
    /// Call under the transition write lock, before the first mutation. Evict on
    /// errors too: a later database may fail after an earlier one committed.
    pub fn invalidate_tenant(&self, dsp: &str, domain: DataDomain) {
        self.invalidate_prepared(dsp, domain, PeopleImpact::MayChange);
    }
    /// Only State's prepared write may preserve the proved tenant's PEOPLE scope. Global
    /// scopes and all other reads retain the same dependency predicate.
    pub(crate) fn invalidate_prepared(&self, dsp: &str, domain: DataDomain, people: PeopleImpact) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| {
                let scope = &entry.scope;
                let unchanged = people == PeopleImpact::Unchanged
                    && scope.read == PEOPLE
                    && scope.dsp.as_deref() == Some(dsp);
                unchanged
                    || !(scope.dsp.as_deref().is_none_or(|tenant| tenant == dsp)
                        && evicts(scope.read, domain))
            });
        }
    }
    /// Evicts everything one DSP's cached reads hold, whatever its kind, and every read the
    /// DSPs share, as the listings: what a write to that DSP's data could change.
    pub fn invalidate_dsp(&self, dsp: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| {
                entry
                    .scope
                    .dsp
                    .as_deref()
                    .is_some_and(|tenant| tenant != dsp)
            });
        }
    }
    /// Expired invitations can change the owner shown in any user's DSP listing.
    pub fn invalidate_listings(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| entry.scope != Scope::listings());
        }
    }
    pub fn read<T: Serialize + Clone + Send + 'static>(
        &self,
        scope: Scope,
        key: String,
        revision: u64,
        load: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.read_sized(scope, key, revision, load, |value| {
            Ok(serde_json::to_vec(value)?.len().saturating_mul(2))
        })
    }
    /// Retain one encoded response; Bytes clones share its allocation across viewers.
    pub fn json<T: Serialize>(
        &self,
        scope: Scope,
        key: String,
        revision: u64,
        load: impl FnOnce() -> Result<T>,
    ) -> Result<axum::body::Bytes> {
        self.read_sized(
            scope,
            key,
            revision,
            || Ok(axum::body::Bytes::from(serde_json::to_vec(&load()?)?)),
            |bytes| Ok(bytes.len()),
        )
    }
    /// A hit needs no per-key turn or allocation. Hold the cache mutex only for its lookup.
    fn cached<T: Clone + Send + 'static>(
        &self,
        scope: &Scope,
        key: &str,
        revision: u64,
    ) -> Option<T> {
        if let Ok(mut entries) = self.entries.lock() {
            let now = Instant::now();
            entries.retain(|entry| entry.revision == revision && entry.until > now);
            if let Some(index) = entries
                .iter()
                .position(|entry| entry.key == key && &entry.scope == scope)
            {
                let entry = entries.remove(index).unwrap();
                let result = entry.value.downcast_ref::<T>().cloned();
                entries.push_back(entry);
                if let Some(result) = result {
                    return Some(result);
                }
            }
        }
        None
    }
    fn read_sized<T: Clone + Send + 'static>(
        &self,
        scope: Scope,
        key: String,
        revision: u64,
        load: impl FnOnce() -> Result<T>,
        size: impl FnOnce(&T) -> Result<usize>,
    ) -> Result<T> {
        if let Some(value) = self.cached(&scope, &key, revision) {
            return Ok(value);
        }
        let value = load()?;
        let bytes = size(&value)?;
        if bytes <= LARGEST
            && let Ok(mut entries) = self.entries.lock()
        {
            entries.retain(|entry| {
                (entry.key != key || entry.scope != scope) && entry.revision == revision
            });
            while entries.len() >= ENTRIES
                || entries.iter().map(|entry| entry.bytes).sum::<usize>() + bytes > KEPT
            {
                entries.pop_front();
            }
            entries.push_back(Entry {
                until: Instant::now() + lifetime(scope.read),
                key,
                scope,
                revision,
                value: Box::new(value.clone()),
                bytes,
            });
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "../tests/backend/cache.rs"]
mod tests;
