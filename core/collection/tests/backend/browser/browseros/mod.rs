use super::*;
#[test]
fn a_failed_start_names_the_workers_own_reason() {
    assert_eq!(
        start_failure(Some(&json!({"error":"browser_display_failed"})), ""),
        "browser_display_failed"
    );
    assert_eq!(start_failure(None, ""), "worker exited before reporting");
    // When the worker never reports, the sandbox's own complaint explains why.
    assert_eq!(
        start_failure(None, "bwrap: setting up uid map: Permission denied"),
        "worker exited before reporting: bwrap: setting up uid map: Permission denied"
    );
    // A worker that did report is the authority; the sandbox's echo of it adds nothing.
    assert_eq!(
        start_failure(
            Some(&json!({"error":"browser_lost"})),
            "core.failed browser_lost"
        ),
        "browser_lost"
    );
    // An unexpected frame is kept, bounded, rather than replaced by a generic failure.
    assert_eq!(
        start_failure(Some(&json!({"ready":false})), ""),
        "unexpected frame {\"ready\":false}"
    );
    assert_eq!(
        start_failure(
            Some(&json!({"error":"browser_lost","detail":"bwrap: no permission"})),
            ""
        ),
        "browser_lost: bwrap: no permission"
    );
    assert_eq!(
        start_failure(Some(&json!({"error":""})), ""),
        "unexpected frame {\"error\":\"\"}"
    );
    assert_eq!(
        start_failure(Some(&json!({"error":"x".repeat(400)})), "").len(),
        400
    );
    assert!(start_failure(Some(&json!({"note":"y".repeat(400)})), "").len() <= 200);
}
