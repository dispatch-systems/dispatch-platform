use super::*;

#[test]
fn prompt_arguments_are_bounded_and_named() {
    assert!(prompt_text("daily_summary", &Map::new()).unwrap().is_some());
    assert!(
        prompt_text(
            "daily_summary",
            &Map::from_iter([("other".into(), json!("x"))])
        )
        .is_err()
    );
    assert!(
        prompt_text(
            "driver_review",
            &Map::from_iter([("driver".into(), json!("x".repeat(201)))])
        )
        .is_err()
    );
    assert!(prompt_text("missing", &Map::new()).unwrap().is_none());
}
