use super::*;

#[test]
fn a_chunk_crossing_the_body_limit_is_refused_before_copying() {
    let mut bytes = b"ab".to_vec();
    assert!(append_within(&mut bytes, b"cd", 4, "http_too_large").is_ok());
    assert_eq!(bytes, b"abcd");
    let capacity = bytes.capacity();
    assert!(matches!(
        append_within(&mut bytes, b"e", 4, "http_too_large"),
        Err(Refusal::Unreadable("http_too_large"))
    ));
    assert_eq!(bytes, b"abcd");
    assert_eq!(bytes.capacity(), capacity);
}
