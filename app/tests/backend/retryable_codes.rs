//! The codes a job retries: core's and every registered collector's.
use dispatch_core::foundation::error::{Code, Error};
#[test]
fn the_retryable_codes_are_exactly_the_ones_jobs_always_retried() {
    let retryable: Vec<_> = Code::retryable().map(|code| code.as_str()).collect();
    assert_eq!(
        retryable,
        [
            "browser_lost",
            "browser_closed",
            "browser_command_timeout",
            "provider_timeout",
            "provider_unavailable",
            "provider_navigation_timeout",
            "provider_content_timeout",
            "provider_response_unreadable",
            "cortex_source_changed",
            "dvic_source_changed",
            "cortex_content_incomplete",
            "scorecard_api_unreadable",
        ]
    );
    for code in Code::all() {
        assert_eq!(Code::parse(code.as_str()), Some(code));
        assert_eq!(code.is_retryable(), retryable.contains(&code.as_str()));
    }
    assert!(Error::new("browser_lost", 503).is_retryable());
    assert!(!Error::new("queue_full", 429).is_retryable());
    assert!(!Code::text_is_any("queue_full", Code::RETRYABLE));
}
