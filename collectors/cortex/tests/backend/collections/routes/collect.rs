use super::*;
use std::sync::Arc;

#[test]
fn concurrent_lanes_cannot_overbook_the_capture_budget() {
    let budget = Arc::new(
        CaptureBudget::new(crate::collectors::cortex::routes::MAX_CAPTURE_BYTES - 1).unwrap(),
    );
    let results = std::thread::scope(|scope| {
        let first = scope.spawn({
            let budget = budget.clone();
            move || budget.add(1)
        });
        let second = scope.spawn({
            let budget = budget.clone();
            move || budget.add(1)
        });
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(results.iter().any(|result| result.is_err()));
}
#[test]
fn an_itinerary_is_owned_by_its_driver_without_reading_the_rest() {
    let text = r#"{"itineraryDetails":{"stops":[{"tasks":[]}],"itineraryId":"a","transporterId":"t"},"addresses":[]}"#;
    assert!(owned_by(text, "a", "t"));
    assert!(!owned_by(text, "a", "other"));
    assert!(!owned_by(text, "b", "t"));
    assert!(!owned_by("{}", "a", "t"));
}
#[test]
fn a_tab_pauses_only_its_data_responses() {
    let value = patterns(&[ITINERARY]);
    let patterns = value["patterns"].as_array().unwrap();
    assert_eq!(patterns.len(), 2);
    assert!(patterns.iter().all(|p| p["requestStage"] == "Response"));
}
