//! What the read cache evicts for every registered feature's writes.
use dispatch_core::{Result, server::cache::*};
fn meals(dsp: &str) -> Scope {
    Scope::tenant("meals", dsp)
}
#[tokio::test]
async fn bookkeeping_preserves_cache_and_failed_scoped_or_unknown_writes_invalidate() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::Ordering;
    crate::install();
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dispatch_core::db::private_dir(root.path()).unwrap();
    let mut config = dispatch_core::foundation::config::Config::load().unwrap();
    config.root = root.path().into();
    config.fixture = true;
    config.development = true;
    config.environment = "preview".into();
    let state = dispatch_core::State::new(config).unwrap();
    state
        .run(|db| {
            for dsp in ["a", "b"] {
                db.platform.exec(
                    "INSERT INTO dsps(id,name,environment,status,timezone,created_at) \
                VALUES (?,'Cache test','preview','provisioning','UTC',?)",
                    [dsp, &dispatch_core::db::iso()],
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
            Err(dispatch_core::Error::new("after_write", 500))
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
            Err(dispatch_core::Error::new("after_write", 500))
        })
        .await;
    assert!(failed.is_err());
    assert_eq!(cached("a").await.unwrap(), "America/Los_Angeles");
    assert_eq!(cached("b").await.unwrap(), "America/Los_Angeles");
    assert_eq!(loads.load(Ordering::Relaxed), 5);
}
