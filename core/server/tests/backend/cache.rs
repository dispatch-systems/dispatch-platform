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
