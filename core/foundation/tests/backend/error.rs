use super::*;
#[tokio::test]
async fn storage_errors_keep_their_cause_and_their_wire_code() {
    use std::error::Error as _;
    let error = Error::from(std::io::Error::other("disk detail"));
    assert_eq!(
        (error.code.as_str(), error.status),
        ("operation_failed", 500)
    );
    assert_eq!(error.source().unwrap().to_string(), "disk detail");
    assert!(Error::new("invalid_input", 400).source().is_none());
    let response = error.into_response();
    assert_eq!(response.status().as_u16(), 500);
    let bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::json!({"error":"operation_failed","message":"operation failed"})
    );
}
