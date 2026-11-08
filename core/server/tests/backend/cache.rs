use super::*;

#[tokio::test]
async fn duplicate_reads_wait_but_other_tenants_and_cancelled_waiters_do_not() {
    let turns = Arc::new(ReadTurns::default());
    let first = turns.enter("a:meals:day".into()).await.unwrap();
    let mut duplicate = Box::pin(turns.enter("a:meals:day".into()));
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(duplicate.as_mut().poll(cx).is_pending()))
            .await
    );
    let other = turns.enter("b:meals:day".into()).await.unwrap();
    drop(duplicate);
    drop(first);
    let next = turns.enter("a:meals:day".into()).await.unwrap();
    drop((next, other));
    assert_eq!(turns.slots.available_permits(), 64);
}
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
            Ok("x".repeat(super::LARGEST))
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

#[test]
fn shared_misses_reuse_one_calculation_while_another_tenant_can_finish() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };
    let cache = Arc::new(ReadCache::default());
    let loads = Arc::new(AtomicUsize::new(0));
    let (started, loading) = mpsc::channel();
    let (release, released) = mpsc::channel();
    std::thread::scope(|threads| {
        let (first_cache, first_loads) = (cache.clone(), loads.clone());
        let first = threads.spawn(move || {
            first_cache
                .shared(meals("a"), "people".into(), 1, || {
                    first_loads.fetch_add(1, Ordering::SeqCst);
                    started.send(()).unwrap();
                    released.recv().unwrap();
                    Ok(7)
                })
                .unwrap()
        });
        loading.recv().unwrap();
        let (ready, readers) = mpsc::channel();
        let duplicates: Vec<_> = (0..8)
            .map(|_| {
                let (cache, loads, ready) = (cache.clone(), loads.clone(), ready.clone());
                threads.spawn(move || {
                    ready.send(()).unwrap();
                    cache
                        .shared(meals("a"), "people".into(), 1, || {
                            loads.fetch_add(1, Ordering::SeqCst);
                            Ok(7)
                        })
                        .unwrap()
                })
            })
            .collect();
        for _ in 0..8 {
            readers.recv().unwrap();
        }
        // The first calculation is still held. Another DSP finishes before it is released.
        assert_eq!(
            *cache
                .shared(meals("b"), "people".into(), 1, || Ok(9))
                .unwrap(),
            9
        );
        release.send(()).unwrap();
        let first = first.join().unwrap();
        for reader in duplicates {
            let value = reader.join().unwrap();
            assert!(Arc::ptr_eq(&first, &value));
        }
    });
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[test]
fn a_failed_or_panicking_shared_calculation_leaves_the_next_read_free_to_retry() {
    let cache = ReadCache::default();
    assert!(
        cache
            .shared::<i32>(meals("a"), "people".into(), 1, || Err(Error::new(
                "failed", 500
            )))
            .is_err()
    );
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cache.shared::<i32>(meals("a"), "people".into(), 1, || {
            panic!("calculation failed")
        })
    }));
    assert!(panic.is_err());
    assert_eq!(
        *cache
            .shared(meals("a"), "people".into(), 1, || Ok(7))
            .unwrap(),
        7
    );
    assert_eq!(
        *cache
            .shared(meals("a"), "people".into(), 2, || Ok(11))
            .unwrap(),
        11
    );
}
