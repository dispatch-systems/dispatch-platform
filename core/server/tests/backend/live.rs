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

#[test]
fn a_topic_wakes_only_who_waits_on_it_in_that_dsp() {
    let topics = Topics::default();
    let stock = Topic("stock");
    let mut north = topics.subscribe(stock, "north");
    let summit = topics.subscribe(stock, "summit");
    let other = topics.subscribe(Topic("other"), "north");
    topics.notify(stock, "north");
    assert!(north.has_changed().unwrap());
    assert!(!summit.has_changed().unwrap());
    assert!(!other.has_changed().unwrap());
    north.mark_unchanged();
    // Nobody waiting yet is nothing to wake.
    topics.notify(Topic("unheard"), "north");
    topics.notify(stock, "summit");
    assert!(summit.has_changed().unwrap());
    assert!(!north.has_changed().unwrap());
    assert!(!other.has_changed().unwrap());
}
