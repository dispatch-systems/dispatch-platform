use super::*;
#[test]
fn hints_are_scoped_and_lost_history_requires_a_full_refresh() {
    let updates = Updates::new().unwrap();
    let a = updates.subscribe("a");
    let b = updates.subscribe("b");
    let start = updates.token(&a);
    updates.changed("a", CollectionChange::provider("cortex"));
    let next = updates.token(&a);
    assert_eq!(updates.token(&b), start);
    assert_eq!(updates.changes("a", &start, &next)[0].provider, "cortex");
    assert!(updates.changes("a", &next, &next).is_empty());
    assert_eq!(
        updates.changes("a", "old-process:1", &next)[0].provider,
        "all"
    );
    for _ in 0..130 {
        updates.notify("a");
    }
    assert_eq!(
        updates.changes("a", &start, &updates.token(&a))[0].provider,
        "all"
    );
}
