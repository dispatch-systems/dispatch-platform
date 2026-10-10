use super::*;

#[test]
fn endpoint_and_global_bursts_are_bounded_without_a_database() {
    let limits = Limits::default();
    let at = 1_790_000_000_000;
    for _ in 0..120 {
        limits.admit_at(Endpoint::Token, "one", at).unwrap();
    }
    assert_eq!(
        limits
            .admit_at(Endpoint::Token, "one", at + WINDOW - 1)
            .unwrap_err()
            .code,
        "rate_limited"
    );
    // Other sources may use the remaining global allowance, but no more than it.
    for source in 0..480 {
        limits
            .admit_at(Endpoint::Token, &format!("source-{source}"), at)
            .unwrap();
    }
    assert_eq!(
        limits
            .admit_at(Endpoint::Token, "last", at)
            .unwrap_err()
            .code,
        "rate_limited"
    );
    limits
        .admit_at(Endpoint::Token, "one", at + WINDOW)
        .unwrap();
}

#[test]
fn repeated_refusals_are_sampled_and_reset() {
    let limits = Limits::default();
    let at = 1_790_000_000_000;
    let sampled: Vec<u32> = (0..10)
        .filter_map(|_| limits.sample_refusal_at("token", "invalid_grant", at))
        .collect();
    assert_eq!(sampled, [1, 2, 3, 4, 5, 8]);
    assert_eq!(
        limits.sample_refusal_at("token", "invalid_grant", at + WINDOW),
        Some(1)
    );
}
