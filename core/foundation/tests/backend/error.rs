use super::*;
#[test]
fn storage_errors_keep_their_cause_and_their_wire_code() {
    use std::error::Error as _;
    let error = Error::from(std::io::Error::other("disk detail"));
    assert_eq!(
        (error.code.as_str(), error.status),
        ("operation_failed", 500)
    );
    assert_eq!(error.source().unwrap().to_string(), "disk detail");
    assert!(Error::new("invalid_input", 400).source().is_none());
}
