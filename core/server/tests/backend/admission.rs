use super::*;

#[tokio::test]
async fn duplicate_reads_wait_but_other_tenants_and_cancelled_waiters_do_not() {
    let turns = Arc::new(KeyedTurns::new(Origins::READS));
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
