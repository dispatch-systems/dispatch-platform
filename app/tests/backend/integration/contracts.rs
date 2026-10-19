use dispatch_core::{
    accounts::api::types::DspProfile,
    collection::{api::metrics::JobMetrics, metrics::Recorder},
};
use serde_json::{Value, json};

#[test]
fn recorded_diagnostics_round_trip_and_older_metrics_do_not_invent_page_reads() {
    let recorder = Recorder::new(&json!({"attempt":1,"started_at":"2026-09-21T00:00:00Z"}));
    recorder.page_start(1, 1);
    recorder.page_stage(1, "content");
    recorder.page_loading(1, Some(2), "interactive");
    recorder.page_finish(1, None);
    recorder.finish("succeeded", None);
    let original = serde_json::to_value(recorder.snapshot()).unwrap();
    let parsed: JobMetrics = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    let mut historical = original.clone();
    for name in ["detail", "itineraries", "meals", "pageReads"] {
        historical.as_object_mut().unwrap().remove(name);
    }
    let parsed: JobMetrics = serde_json::from_value(historical).unwrap();
    assert!(parsed.page_reads.is_none());
    assert!(parsed.itineraries.is_none() && parsed.meals.is_none());
    assert!(
        serde_json::to_value(parsed)
            .unwrap()
            .get("pageReads")
            .is_none()
    );
    for pointer in [
        "/outcome",
        "/phase",
        "/pageReads/slowest/0/stage",
        "/pageReads/slowest/0/documentState",
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("not_a_state");
        assert!(
            serde_json::from_value::<JobMetrics>(invalid).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn old_page_read_counters_default_without_losing_recorded_timings() {
    let recorder = Recorder::new(&json!({"attempt":1}));
    let mut record = serde_json::to_value(recorder.snapshot()).unwrap();
    let reads = record["pageReads"].as_object_mut().unwrap();
    for name in ["resumed", "earlyReady", "direct", "spotChecked"] {
        reads.remove(name);
    }
    reads.insert("completed".into(), json!(7));
    reads.insert("totalMs".into(), json!(1234));
    let parsed: JobMetrics = serde_json::from_value(record).unwrap();
    let reads = parsed.page_reads.unwrap();
    assert_eq!(
        (reads.completed, reads.total_ms, reads.resumed, reads.direct),
        (7, 1234, 0, 0)
    );
}

#[test]
fn partial_profiles_keep_defaults_and_reject_mistyped_flags() {
    let profile: DspProfile = serde_json::from_value(json!({"stationCode":"DEMO1"})).unwrap();
    assert_eq!(profile.station_code, "DEMO1");
    assert!(!profile.setup_required && !profile.removed && !profile.support_visible);
    assert!(profile.abbreviation.is_empty());
    for value in [json!("false"), json!(0), Value::Null] {
        assert!(serde_json::from_value::<DspProfile>(json!({"removed":value})).is_err());
    }
}
