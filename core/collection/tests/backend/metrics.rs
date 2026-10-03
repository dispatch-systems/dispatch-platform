use super::*;
use serde_json::json;
#[test]
fn page_history_is_bounded_and_interruption_freezes_active_reads() {
    let recorder = Recorder::new(&serde_json::json!({"attempt":1}));
    for ordinal in 1..=20 {
        recorder.page_start(ordinal, 1);
        recorder.page_stage(ordinal, "content");
        recorder.page_finish(ordinal, Some("provider_content_missing"));
        recorder.page_start(ordinal, 2);
        recorder.page_stage(ordinal, "extraction");
        recorder.page_finish(ordinal, None);
    }
    recorder.page_start(21, 1);
    recorder.finish("cancelled", Some("job_cancelled"));
    let before = serde_json::to_value(recorder.snapshot()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    assert_eq!(before, serde_json::to_value(recorder.snapshot()).unwrap());
    let reads = &before["pageReads"];
    assert_eq!(reads["completed"], 20);
    assert_eq!(reads["recovered"], 20);
    assert_eq!(reads["retries"], 20);
    assert_eq!(reads["failures"].as_array().unwrap().len(), 8);
    assert_eq!(reads["slowest"].as_array().unwrap().len(), 5);
    assert_eq!(reads["active"][0]["ordinal"], 21);
}
#[test]
fn failure_detail_keeps_only_fixed_labels_and_clears_on_success() {
    let detail = |recorder: &Recorder| {
        serde_json::to_value(recorder.snapshot()).unwrap()["detail"].clone()
    };
    let recorder = Recorder::new(&json!({"attempt":1}));
    recorder.detail("summaries_loading");
    assert_eq!(detail(&recorder), "summaries_loading");
    for label in ["", "Station TST1", "https://example.test", &"a".repeat(49)] {
        recorder.detail(label);
        assert_eq!(detail(&recorder), Value::Null, "{label}");
    }
    recorder.detail("path");
    assert_eq!(summary(&recorder.snapshot())["detail"], "path");
    let failed = Recorder::new(&json!({"attempt":1}));
    failed.detail("path");
    failed.finish("failed", Some("cortex_content_incomplete"));
    assert_eq!(detail(&failed), "path");
    recorder.finish("succeeded", None);
    assert_eq!(detail(&recorder), Value::Null);
}
#[test]
fn partial_samples_do_not_claim_a_complete_shared_memory_peak() {
    let recorder =
        Recorder::new(&json!({"attempt":1,"started_at":db::iso(),"available_at":db::now()}));
    recorder.observe(Memory {
        rss: 100,
        pss: 70,
        private: 50,
        complete: true,
    });
    recorder.observe(Memory {
        rss: 200,
        pss: 150,
        private: 100,
        complete: false,
    });
    let value = recorder.snapshot();
    assert_eq!(value.peak_rss_bytes, Some(200));
    assert_eq!(value.peak_pss_bytes, Some(70));
    assert_eq!(value.peak_private_bytes, Some(50));
    assert_eq!(value.incomplete_memory_samples, 1);
    assert_eq!(value.authentication_ms, None);
    recorder.phase(Phase::Collection);
    recorder.finish("cancelled", Some("job_cancelled"));
    assert_eq!(recorder.snapshot().outcome, JobOutcome::Cancelled);
    assert!(recorder.snapshot().collection_ms.is_some());
    assert!(recorder.snapshot().publication_ms.is_none());
}
