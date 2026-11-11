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

#[test]
fn retry_after_accepts_seconds_and_http_dates_without_guessing_a_delay() {
    use reqwest::header::HeaderValue;
    for (text, expected) in [
        ("17", Some(Duration::from_secs(17))),
        ("0", Some(Duration::ZERO)),
        ("Wed, 21 Oct 2015 07:28:00 GMT", Some(Duration::ZERO)),
        ("-1", None),
        ("later", None),
    ] {
        assert_eq!(retry_after(Some(&HeaderValue::from_static(text))), expected);
    }
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    let date = HeaderValue::from_str(&future.to_rfc2822()).unwrap();
    assert!(retry_after(Some(&date)).is_some_and(|delay| !delay.is_zero()));
    assert_eq!(retry_after(None), None);
}
