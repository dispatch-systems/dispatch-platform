//! Bounded derived reads shared across database workers. Call only after authorization,
//! while holding State's transition read lock. Unknown writes advance the global
//! revision; reviewed writes evict the scopes that depend on their changed data.
use crate::{Result, manifest::registry};
use serde::Serialize;
use std::any::Any;
use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

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
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }
    pub fn id(self) -> &'static str {
        self.0
    }
    /// What a finished collection of `kind` writes, as its keeper declares. A keeper that
    /// declares none keeps the conservative tenant-wide dependency set.
    pub fn collection(kind: crate::contracts::JobKind) -> Self {
        registry().keeper(kind.as_str()).domain()
    }
}
/// Core's own domains, beside the features'.
pub const DOMAINS: &[DataDomain] = &[DataDomain::LIVE, DataDomain::TENANT, DataDomain::SCHEDULES];

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
#[derive(Default)]
pub struct ReadCache {
    entries: Mutex<VecDeque<Entry>>,
}
impl ReadCache {
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
        const BYTES: usize = 8 * 1024 * 1024;
        if bytes <= BYTES
            && let Ok(mut entries) = self.entries.lock()
        {
            entries.retain(|entry| {
                (entry.key != key || entry.scope != scope) && entry.revision == revision
            });
            while entries.len() >= 64
                || entries.iter().map(|entry| entry.bytes).sum::<usize>() + bytes > BYTES
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
mod tests {
    use super::*;
    fn meals(dsp: &str) -> Scope {
        Scope::tenant("meals", dsp)
    }
    #[test]
    fn reuses_matching_reads_but_not_another_revision_or_tenant() {
        let cache = ReadCache::default();
        assert_eq!(
            cache.read(meals("a"), "day".into(), 1, || Ok(7)).unwrap(),
            7
        );
        assert_eq!(
            cache
                .read(meals("a"), "day".into(), 1, || -> Result<i32> {
                    panic!("must reuse")
                })
                .unwrap(),
            7
        );
        assert_eq!(
            cache.read(meals("b"), "day".into(), 1, || Ok(9)).unwrap(),
            9
        );
        assert_eq!(
            cache.read(meals("a"), "day".into(), 2, || Ok(10)).unwrap(),
            10
        );
    }
    #[test]
    fn encoded_results_share_storage_and_remain_json() {
        let cache = ReadCache::default();
        let first = cache
            .json(meals("a"), "meals".into(), 1, || {
                Ok(serde_json::json!({"rows":[1,2,3]}))
            })
            .unwrap();
        let second = cache
            .json(meals("a"), "meals".into(), 1, || -> Result<()> {
                panic!("must reuse")
            })
            .unwrap();
        assert_eq!(first.as_ptr(), second.as_ptr());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&second).unwrap(),
            serde_json::json!({"rows":[1,2,3]})
        );
    }
    #[test]
    fn oversized_results_and_errors_are_not_cached() {
        let cache = ReadCache::default();
        cache
            .read(meals("a"), "large".into(), 1, || {
                Ok("x".repeat(8 * 1024 * 1024))
            })
            .unwrap();
        assert!(cache.entries.lock().unwrap().is_empty());
        assert!(
            cache
                .read::<i32>(meals("a"), "error".into(), 1, || Err(crate::Error::new(
                    "failed", 500
                )))
                .is_err()
        );
        assert!(cache.entries.lock().unwrap().is_empty());
    }
    #[test]
    fn dependencies_cover_every_source_without_evicting_other_tenants() {
        for (domain, expected) in [
            (DataDomain::new("paycom"), [false, false, false, true, true]),
            (DataDomain::new("meals"), [true, false, false, true, true]),
            (DataDomain::LIVE, [true, false, true, true, true]),
            (DataDomain::new("routes"), [true, true, false, true, true]),
            (
                DataDomain::new("scorecard"),
                [true, true, false, true, true],
            ),
            (DataDomain::new("dvic"), [true, true, false, true, true]),
            (DataDomain::new("drivers"), [true, false, false, true, true]),
            (DataDomain::TENANT, [false, false, false, true, true]),
            (DataDomain::SCHEDULES, [false, true, true, true, true]),
        ] {
            let cache = ReadCache::default();
            let scopes = [
                Scope::listings(),
                meals("a"),
                Scope::tenant(PEOPLE, "a"),
                meals("b"),
                Scope::tenant(PEOPLE, "b"),
            ];
            for scope in &scopes {
                cache
                    .read(scope.clone(), "same-key".into(), 1, || Ok(7))
                    .unwrap();
            }
            cache.invalidate_tenant("a", domain);
            for (scope, retained) in scopes.into_iter().zip(expected) {
                assert_eq!(
                    cache
                        .read(scope.clone(), "same-key".into(), 1, || Ok(9))
                        .unwrap(),
                    if retained { 7 } else { 9 },
                    "{domain:?}: {scope:?}"
                );
            }
        }
    }
    #[test]
    fn simultaneous_misses_compute_without_serializing_and_store_one_entry() {
        let cache = std::sync::Arc::new(ReadCache::default());
        let loading = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (cache, loading) = (cache.clone(), loading.clone());
                std::thread::spawn(move || {
                    cache
                        .read(meals("a"), "day".into(), 1, || {
                            loading.wait();
                            Ok(7)
                        })
                        .unwrap()
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), 7);
        }
        assert_eq!(cache.entries.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn bookkeeping_preserves_cache_and_failed_scoped_or_unknown_writes_invalidate() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::Ordering;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        crate::db::private_dir(root.path()).unwrap();
        let mut config = crate::config::Config::load().unwrap();
        config.root = root.path().into();
        config.fixture = true;
        config.development = true;
        config.environment = "preview".into();
        let state = crate::State::new(config).unwrap();
        state
            .run(|db| {
                for dsp in ["a", "b"] {
                    db.platform.exec(
                        "INSERT INTO dsps(id,name,environment,status,timezone,created_at) \
                    VALUES (?,'Cache test','preview','provisioning','UTC',?)",
                        [dsp, &crate::db::iso()],
                    )?;
                }
                Ok(())
            })
            .await
            .unwrap();
        let loads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let cached = |dsp: &'static str| {
            let reader = state.clone();
            let loads = loads.clone();
            state.read(move |db| {
                reader.read_cache.read(
                    meals(dsp),
                    "timezone".into(),
                    reader.data_revision.load(Ordering::Relaxed),
                    || {
                        loads.fetch_add(1, Ordering::Relaxed);
                        Ok(db.find_dsp(dsp)?.timezone)
                    },
                )
            })
        };
        assert_eq!(cached("a").await.unwrap(), "UTC");
        assert_eq!(cached("b").await.unwrap(), "UTC");
        state
            .run_bookkeeping(|db| {
                db.platform
                    .exec("DELETE FROM outbox WHERE status='sent'", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(cached("a").await.unwrap(), "UTC");
        assert_eq!(cached("b").await.unwrap(), "UTC");
        assert_eq!(loads.load(Ordering::Relaxed), 2);
        let failed: Result<()> = state
            .run_scoped("a", DataDomain::TENANT, |db| {
                db.platform.exec(
                    "UPDATE dsps SET timezone='America/New_York' WHERE id='a'",
                    [],
                )?;
                Err(crate::Error::new("after_write", 500))
            })
            .await;
        assert!(failed.is_err());
        assert_eq!(cached("a").await.unwrap(), "America/New_York");
        assert_eq!(cached("b").await.unwrap(), "UTC");
        assert_eq!(loads.load(Ordering::Relaxed), 3);
        let failed: Result<()> = state
            .run(|db| {
                db.platform
                    .exec("UPDATE dsps SET timezone='America/Los_Angeles'", [])?;
                Err(crate::Error::new("after_write", 500))
            })
            .await;
        assert!(failed.is_err());
        assert_eq!(cached("a").await.unwrap(), "America/Los_Angeles");
        assert_eq!(cached("b").await.unwrap(), "America/Los_Angeles");
        assert_eq!(loads.load(Ordering::Relaxed), 5);
    }
}
