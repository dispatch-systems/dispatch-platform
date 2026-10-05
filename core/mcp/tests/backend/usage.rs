use super::*;

#[test]
fn a_key_gets_a_rolling_minute_of_calls_then_waits() {
    let usage = Usage::default();
    let at = 1_790_000_000_000;
    for _ in 0..PER_MINUTE {
        usage.admit_at("key_a", "curl", at).unwrap();
    }
    let refused = usage.admit_at("key_a", "curl", at + 1).unwrap_err();
    assert_eq!(refused.code, "rate_limited");
    // A wall-clock minute boundary cannot double the allowed burst. Calls age out only
    // after their own rolling minute, and another key has its own allowance.
    assert_eq!(
        usage
            .admit_at("key_a", "curl", at + 59_999)
            .unwrap_err()
            .code,
        "rate_limited"
    );
    usage.admit_at("key_a", "curl", at + 60_000).unwrap();
    let boundary = Usage::default();
    let minute = at - at.rem_euclid(60_000);
    for _ in 0..PER_MINUTE {
        boundary.admit_at("key_a", "curl", minute + 59_000).unwrap();
    }
    assert_eq!(
        boundary
            .admit_at("key_a", "curl", minute + 60_000)
            .unwrap_err()
            .code,
        "rate_limited"
    );
    usage.admit_at("key_b", "Codex 0.157", at).unwrap();
    let taken = usage.take();
    assert_eq!(taken.len(), 2);
    assert!(usage.take().is_empty());
    assert_eq!(usage.last()["key_b"].1, "Codex 0.157");
    // Writing them down failed: the next take finds them again.
    usage.unsaved(&["key_b".to_owned(), "key_gone".to_owned()]);
    let again = usage.take();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].0, "key_b");
}
