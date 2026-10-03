use super::*;
use serde_json::json;
#[test]
fn missing_or_mistyped_fields_do_not_become_empty_values() {
    for input in [
        json!({"email":"a@example.com"}),
        json!({"email":"a@example.com","password":123}),
        json!({"email":"a@example.com","password":"","extra":true}),
    ] {
        assert!(LoginRequest::parse(&input).is_err());
    }
    assert!(CollectionRequest::parse(&json!({"requestId":"test","date":null}), false).is_err());
    assert!(CollectionRequest::parse(&json!({"requestId":"test"}), true).is_err());
    assert!(
        CollectionRequest::parse(&json!({"requestId":"test","date":"2026-02-30"}), false).is_err()
    );
    assert!(JobStatus::parse("finished").is_none());
    let list = |statuses: &[JobStatus]| {
        let quoted: Vec<_> = statuses
            .iter()
            .map(|s| format!("'{}'", s.as_str()))
            .collect();
        format!("({})", quoted.join(","))
    };
    assert_eq!(
        dispatch_core::job_statuses!(active),
        list(JobStatus::ACTIVE)
    );
    assert_eq!(
        dispatch_core::job_statuses!(leased),
        list(JobStatus::LEASED)
    );
    assert!(serde_json::from_value::<JobStatus>(json!("finished")).is_err());
}
