//! What Slack's passes share: work Slack told of that fails is tried again soon, until it
//! succeeds, or a few times, then left to the passes that catch up on what was missed.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test(start_paused = true)]
async fn work_slack_told_of_that_fails_is_tried_again_soon_a_few_times() {
    let tries = AtomicUsize::new(0);
    let done = retried(|| async {
        match tries.fetch_add(1, Ordering::SeqCst) {
            0 | 1 => Err(Error::new("platform_busy", 503)),
            _ => Ok(()),
        }
    })
    .await;
    assert!(done.is_ok());
    assert_eq!(tries.load(Ordering::SeqCst), 3);
    // Failing each time, it's left, saying why.
    let tries = AtomicUsize::new(0);
    let left = retried(|| async {
        tries.fetch_add(1, Ordering::SeqCst);
        Err(Error::new("platform_busy", 503))
    })
    .await;
    assert_eq!(left.unwrap_err().code, "platform_busy");
    assert!(tries.load(Ordering::SeqCst) > 1);
}
