use super::*;
use futures_util::FutureExt;
#[test]
fn hints_are_scoped_and_lost_history_requires_a_full_refresh() {
    let updates = Updates::new().unwrap();
    let a = updates.subscribe("a");
    let b = updates.subscribe("b");
    let start = updates.token(&a);
    updates.changed("a", "dvic", CollectionChange::provider("cortex"));
    let next = updates.token(&a);
    let any = |_: &str| true;
    assert_eq!(updates.token(&b), start);
    assert_eq!(
        updates.changes("a", &start, &next, any)[0].provider,
        "cortex"
    );
    assert!(updates.changes("a", &next, &next, any).is_empty());
    assert_eq!(
        updates.changes("a", "old-process:1", &next, any)[0].provider,
        "all"
    );
    for _ in 0..130 {
        updates.notify("a");
    }
    assert_eq!(
        updates.changes("a", &start, &updates.token(&a), any)[0].provider,
        "all"
    );
}

#[test]
fn a_reader_hears_only_of_the_collections_it_follows() {
    let updates = Updates::new().unwrap();
    let a = updates.subscribe("a");
    let start = updates.token(&a);
    let mut timecard = CollectionChange::provider("paycom");
    timecard.employee_code = Some("E001".into());
    updates.changed("a", "paycom.timecards", timecard);
    updates.changed("a", "dvic", CollectionChange::provider("cortex"));
    updates.notify("a");
    let through = updates.token(&a);
    let heard = updates.changes("a", &start, &through, |kind| kind == "dvic");
    let providers: Vec<_> = heard
        .iter()
        .map(|change| change.provider.as_str())
        .collect();
    // Another feature's collection, and the employee it names, stay with its own readers.
    assert_eq!(providers, ["cortex", "all"]);
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

#[tokio::test]
async fn keyed_topics_keep_unrelated_readers_quiet_and_broad_wakes_reach_every_reader() {
    let topics = Topics::default();
    let topic = Topic("stock");
    let mut default = topics.subscribe(topic, "north");
    let mut first = topics.subscribe_scoped(topic, "north", "one");
    let mut duplicate = topics.subscribe_scoped(topic, "north", "one");
    let mut others: Vec<_> = (2..=20)
        .map(|n| topics.subscribe_scoped(topic, "north", &n.to_string()))
        .collect();
    let mut tenant = topics.subscribe_scoped(topic, "south", "one");
    let mut other_topic = topics.subscribe_scoped(Topic("other"), "north", "one");
    // The write lands after subscription and before the first wait: no lost wake.
    topics.notify_scoped(topic, "north", "one");
    assert!(first.changed().now_or_never().is_some());
    assert!(duplicate.changed().now_or_never().is_some());
    assert!(!default.has_changed().unwrap());
    for other in &mut others {
        assert!(other.changed().now_or_never().is_none());
    }
    assert!(tenant.changed().now_or_never().is_none());
    assert!(other_topic.changed().now_or_never().is_none());
    // A cancelled changed() future leaves its listener able to receive the next wake.
    assert!(first.changed().now_or_never().is_none());
    topics.notify(topic, "north");
    assert!(default.changed().await.is_ok());
    assert!(first.changed().await.is_ok());
    assert!(duplicate.changed().await.is_ok());
    for other in &mut others {
        assert!(other.changed().await.is_ok());
    }
    assert!(tenant.changed().now_or_never().is_none());
    assert!(other_topic.changed().now_or_never().is_none());
}

#[tokio::test]
async fn keyed_channels_are_reclaimed_on_cancellation_forget_and_server_close() {
    let topics = Topics::default();
    let topic = Topic("stock");
    // Churn cannot retain one key per historical request/device.
    for n in 0..100 {
        let mut listener = topics.subscribe_scoped(topic, "north", &n.to_string());
        assert!(listener.changed().now_or_never().is_none());
        drop(listener);
        topics.notify_scoped(topic, "north", &n.to_string());
        assert!(topics.scoped.lock().unwrap().is_empty());
    }
    let first = topics.subscribe_scoped(topic, "north", "one");
    let mut duplicate = topics.subscribe_scoped(topic, "north", "one");
    drop(first);
    topics.notify_scoped(topic, "north", "one");
    assert!(duplicate.changed().await.is_ok());
    let mut default = topics.subscribe(topic, "north");
    let mut south = topics.subscribe_scoped(topic, "south", "one");
    topics.forget_dsp("north");
    assert!(duplicate.changed().await.is_err());
    assert!(default.changed().await.is_err());
    assert!(south.changed().now_or_never().is_none());
    // An old listener dropped after replacement must not remove the replacement's channel.
    let mut replacement = topics.subscribe_scoped(topic, "north", "one");
    drop(duplicate);
    topics.notify_scoped(topic, "north", "one");
    assert!(replacement.changed().await.is_ok());
    drop(replacement);
    assert!(
        !topics
            .scoped
            .lock()
            .unwrap()
            .contains_key(&(topic, "north".into()))
    );
    // Listeners keep only a weak registry reference, so server close closes all receivers.
    drop(topics);
    assert!(south.changed().await.is_err());
}
