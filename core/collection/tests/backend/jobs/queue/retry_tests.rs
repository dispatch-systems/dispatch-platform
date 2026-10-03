use super::*;
#[test]
fn retries_are_bounded_staggered_and_stable() {
    let values = (0..100)
        .map(|i| retry_delay(&format!("job-{i}"), 1))
        .collect::<std::collections::HashSet<_>>();
    assert!(values.len() > 90);
    assert!(values.iter().all(|delay| (60000..=90000).contains(delay)));
    let delay = retry_delay("job-test", 2);
    assert!((120000..=180000).contains(&delay));
    assert_eq!(delay, retry_delay("job-test", 2));
}
