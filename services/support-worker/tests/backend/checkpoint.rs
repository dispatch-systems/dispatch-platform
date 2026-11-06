use super::Checkpoint;
use serde_json::{Value, json};

#[test]
fn worker_progress_retains_unknown_checkpoint_state_across_resume() {
    let checkpoint = Checkpoint::new(json!({
        "phase":"queued", "headSha":"verified-head", "prNumber":42,
        "prUrl":"https://github.example/pull/42", "failure":"check failed", "lastHandledRun":17,
        "analysisStage":"isolated", "investigationAttempted":true, "usableInvestigation":true,
        "browserAttempted":true, "liveBrowserAttempted":true, "liveBrowserObserved":false,
        "routingDecision":{"route":"source"}, "priorAnalysis":{"nextStep":"browser"},
        "futureState":{"cursor":["page",null,3],"complete":false}
    }));
    // This is the same merge sent to the platform before the live checkpoint is replaced.
    let next = checkpoint.merged(&json!({"phase":"build","failure":null}));
    assert_eq!(
        next["futureState"],
        json!({"cursor":["page",null,3],"complete":false})
    );
    assert_eq!(checkpoint.failure(), "check failed");
    assert!(checkpoint.published_pr().queued);

    let resumed =
        Checkpoint::new(serde_json::from_slice(&serde_json::to_vec(&next).unwrap()).unwrap());
    assert_eq!(resumed.saved()["phase"], "build");
    assert_eq!(resumed.saved()["failure"], Value::Null);
    assert_eq!(resumed.failure(), "");
    assert_eq!(resumed.last_handled_run(), 17);
    let published = resumed.published_pr();
    assert!(!published.queued);
    assert_eq!(published.head_sha.as_deref(), Some("verified-head"));
    assert_eq!(published.number, Some(42));
    assert_eq!(
        published.url.as_deref(),
        Some("https://github.example/pull/42")
    );
    let analysis = resumed.analysis_progress();
    assert_eq!(analysis.stage.as_deref(), Some("isolated"));
    assert!(analysis.attempted && analysis.usable && analysis.browser_attempted);
    assert!(analysis.live_browser_attempted);
    assert!(!analysis.live_browser_observed);
    assert_eq!(resumed.routing_decision(), Some(json!({"route":"source"})));
    assert_eq!(
        resumed.prior_analysis(),
        Some(json!({"nextStep":"browser"}))
    );
}

#[test]
fn legacy_worker_checkpoints_keep_permissive_defaults_and_raw_values() {
    for raw in [
        Value::Null,
        json!(true),
        json!(3),
        json!("legacy"),
        json!([]),
        json!({}),
        json!({
            "analysisStage":false, "investigationAttempted":"true", "usableInvestigation":1,
            "browserAttempted":null, "liveBrowserAttempted":[], "liveBrowserObserved":{},
            "phase":null, "headSha":42, "prNumber":"42", "prUrl":true,
            "failure":false, "lastHandledRun":-1, "routingDecision":null, "priorAnalysis":null
        }),
    ] {
        let checkpoint = Checkpoint::new(raw.clone());
        let analysis = checkpoint.analysis_progress();
        assert!(analysis.stage.is_none());
        assert!(!analysis.attempted && !analysis.usable && !analysis.browser_attempted);
        assert!(!analysis.live_browser_attempted && !analysis.live_browser_observed);
        let published = checkpoint.published_pr();
        assert!(!published.queued);
        assert!(
            published.head_sha.is_none() && published.number.is_none() && published.url.is_none()
        );
        assert_eq!(checkpoint.failure(), "");
        assert_eq!(checkpoint.last_handled_run(), 0);
        assert_eq!(checkpoint.saved(), raw);
        assert_eq!(
            checkpoint.merged(&Value::Null),
            if raw.is_object() {
                raw.clone()
            } else {
                json!({})
            }
        );
        assert_eq!(
            checkpoint.merged(&json!({"phase":"routing"}))["phase"],
            "routing"
        );
    }
    // Absence and an explicit null remain distinguishable for consumers that decode them.
    assert!(Checkpoint::new(json!({})).routing_decision().is_none());
    assert!(Checkpoint::new(json!({})).prior_analysis().is_none());
    assert_eq!(
        Checkpoint::new(json!({"routingDecision":null})).routing_decision(),
        Some(Value::Null)
    );
    assert_eq!(
        Checkpoint::new(json!({"priorAnalysis":null})).prior_analysis(),
        Some(Value::Null)
    );
    // Cancellation can still find a PR with no URL, even outside the queued phase.
    let published = Checkpoint::new(json!({"phase":"build","prNumber":0,"headSha":"legacy-head"}))
        .published_pr();
    assert_eq!(published.number, Some(0));
    assert_eq!(published.head_sha.as_deref(), Some("legacy-head"));
    assert!(published.url.is_none());
}
