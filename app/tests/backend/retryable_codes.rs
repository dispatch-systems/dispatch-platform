//! The codes a job retries: core's and every registered collector's.
use dispatch_core::foundation::error::Code;
#[test]
fn the_retryable_codes_are_exactly_the_ones_jobs_always_retried() {
    crate::install();
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
            "performance_api_unreadable",
        ]
    );
    assert!(!Code::text_is_any("queue_full", Code::RETRYABLE));
}
