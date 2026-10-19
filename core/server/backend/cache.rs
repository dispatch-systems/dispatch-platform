//! Bounded derived reads shared across database workers. Call only after authorization,
//! while holding State's transition read lock. Unknown writes advance the global
//! revision; reviewed writes evict the scopes that depend on their changed data.
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
        let busy = || Error::new("platform_busy", 503);
        let slot = self.slots.clone().try_acquire_owned().map_err(|_| busy())?;
        let lock = {
            let mut keys = self.keys.lock().map_err(|_| busy())?;
            keys.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = keys.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(tokio::sync::Mutex::new(()));
                keys.insert(key, Arc::downgrade(&lock));
                lock
            }
        };
        let lock = tokio::time::timeout(Duration::from_secs(15), lock.lock_owned())
            .await
            .map_err(|_| busy())?;
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
    at: Instant,
    value: Box<dyn Any + Send>,
    bytes: usize,
}
/// A cached read: what it reads, named by the id its dependencies are declared under
/// (`Cached`), and for which DSP. The DSP listings are every DSP's at once.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}
/// What core declares of its own cached reads.
pub const CACHED: &[Cached] = &[
    Cached {
        read: LISTINGS,
        evicted: Evicted::By(&[DataDomain::TENANT, DataDomain::SCHEDULES]),
    },
    // Driver Match reads publications, never a collection's live pages.
    Cached {
        read: PEOPLE,
        evicted: Evicted::ByAllBut(&[DataDomain::SCHEDULES, DataDomain::LIVE]),
    },
];
fn evicts(read: &str, domain: DataDomain) -> bool {
    registry()
        .cached()
        .filter(|cached| cached.read == read)
        .any(|cached| match cached.evicted {
            Evicted::By(domains) => domains.contains(&domain),
            Evicted::ByAllBut(domains) => !domains.contains(&domain),
        })
}
/// The most the read cache keeps in all, by its estimates: room for the working sets of
/// several busy DSPs at once, each a people directory with its feature's reads beside it, so
/// one DSP's reads don't push another's out.
const KEPT: usize = 64 * 1024 * 1024;
/// The most one result may be, by its estimate, to be kept: no one result fills the cache.
const LARGEST: usize = 16 * 1024 * 1024;
/// The most results the read cache keeps.
const ENTRIES: usize = 256;
#[derive(Default)]
pub struct ReadCache {
    entries: Mutex<VecDeque<Entry>>,
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
        self.read_sized(
            scope,
            key,
            revision,
            || Ok(Arc::new(load()?)),
            |value| Ok(serde_json::to_vec(value.as_ref())?.len().saturating_mul(2)),
        )
    }
    /// Call under the transition write lock, before the first mutation. Evict on
    /// errors too: a later database may fail after an earlier one committed.
    pub fn invalidate_tenant(&self, dsp: &str, domain: DataDomain) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| {
                let scope = &entry.scope;
                !(scope.dsp.as_deref().is_none_or(|tenant| tenant == dsp)
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
    fn read_sized<T: Clone + Send + 'static>(
        &self,
        scope: Scope,
        key: String,
        revision: u64,
        load: impl FnOnce() -> Result<T>,
        size: impl FnOnce(&T) -> Result<usize>,
    ) -> Result<T> {
        // Do not serialize unrelated readers behind an expensive calculation.
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| {
                entry.revision == revision && entry.at.elapsed() < Duration::from_secs(30)
            });
            if let Some(index) = entries
                .iter()
                .position(|entry| entry.key == key && entry.scope == scope)
            {
                let entry = entries.remove(index).unwrap();
                let result = entry.value.downcast_ref::<T>().cloned();
                entries.push_back(entry);
                if let Some(result) = result {
                    return Ok(result);
                }
            }
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
                key,
                scope,
                revision,
                at: Instant::now(),
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
