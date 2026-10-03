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
