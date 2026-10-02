//! Bounded derived reads shared across database workers. Call only after authorization,
//! while holding State's transition read lock. Unknown writes advance the global
//! revision; reviewed writes evict the scopes that depend on their changed data.
use crate::Result;
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
/// The full dependency set of a derived read, independent of its response key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Includes memberships, features, profile, connections and collection dates.
    Listings,
    /// Paycom (including live pages), Cortex meals, driver links and preferences.
    Meals(String),
    /// Driver decisions/preferences and identities/activity from every collection.
    People(String),
}
/// A reviewed business-data mutation. Use State::run for anything not covered here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDomain {
    Paycom,
    Meals,
    /// Temporary Paycom pages/Cortex itineraries; Driver Match reads publications.
    Live,
    Routes,
    Scorecard,
    Dvic,
    Drivers,
    /// Profile, features, connections, permissions or other tenant-wide settings.
    Tenant,
    /// Only scheduled collection dates, also shown in session DSP listings.
    Schedules,
}
impl DataDomain {
    /// A new registered collection keeps the conservative tenant-wide dependency set.
    pub fn collection(kind: crate::contracts::JobKind) -> Self {
        match kind.as_str() {
            "paycom.collect" => Self::Paycom,
            "cortex.meal_breaks.collect" => Self::Meals,
            "cortex.routes.collect" => Self::Routes,
            "cortex.scorecard.collect" => Self::Scorecard,
            "cortex.dvic.collect" => Self::Dvic,
            _ => Self::Tenant,
        }
    }
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
            entries.retain(|entry| !match &entry.scope {
                Scope::Listings => matches!(
                    domain,
                    DataDomain::Paycom | DataDomain::Tenant | DataDomain::Schedules
                ),
                Scope::Meals(tenant) => {
                    tenant == dsp
                        && matches!(
                            domain,
                            DataDomain::Paycom
                                | DataDomain::Meals
                                | DataDomain::Live
                                | DataDomain::Drivers
                                | DataDomain::Tenant
                        )
                }
                Scope::People(tenant) => {
                    tenant == dsp && !matches!(domain, DataDomain::Schedules | DataDomain::Live)
                }
            });
        }
    }
    /// Expired invitations can change the owner shown in any user's DSP listing.
    pub fn invalidate_listings(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| entry.scope != Scope::Listings);
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
        Scope::Meals(dsp.into())
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
            (DataDomain::Paycom, [false, false, false, true, true]),
            (DataDomain::Meals, [true, false, false, true, true]),
            (DataDomain::Live, [true, false, true, true, true]),
            (DataDomain::Routes, [true, true, false, true, true]),
            (DataDomain::Scorecard, [true, true, false, true, true]),
            (DataDomain::Dvic, [true, true, false, true, true]),
            (DataDomain::Drivers, [true, false, false, true, true]),
            (DataDomain::Tenant, [false, false, false, true, true]),
            (DataDomain::Schedules, [false, true, true, true, true]),
        ] {
            let cache = ReadCache::default();
            let scopes = [
                Scope::Listings,
                meals("a"),
                Scope::People("a".into()),
                meals("b"),
                Scope::People("b".into()),
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
            .run_scoped("a", DataDomain::Tenant, |db| {
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
