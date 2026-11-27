use super::*;
use jiff::ToSpan;

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
    let future = jiff::Timestamp::now() + 1.hours();
    let written = future.strftime("%a, %d %b %Y %H:%M:%S +0000").to_string();
    let date = HeaderValue::from_str(&written).unwrap();
    assert!(retry_after(Some(&date)).is_some_and(|delay| !delay.is_zero()));
    assert_eq!(retry_after(None), None);
}

// A cookie a reply sets again replaces what an earlier reply set of it, so a client kept long
// holds no more than the cookies it was sent; one sent expired is kept as removed, and one
// for a domain the address isn't in is never kept.
#[test]
fn a_cookie_set_again_replaces_what_was_set_of_it() {
    use reqwest::{cookie::CookieStore, header::HeaderValue};
    let carried = Carried {
        jar: reqwest::cookie::Jar::default(),
        set: Mutex::default(),
    };
    let url = Url::parse("https://logistics.amazon.com/operations/execution").unwrap();
    for value in ["one", "two", "three"] {
        let header = HeaderValue::from_str(&format!(
            "session-token={value}; Domain=.amazon.com; Path=/"
        ))
        .unwrap();
        carried.set_cookies(&mut [&header].into_iter(), &url);
    }
    let removed = HeaderValue::from_static("x-old=1; Domain=.amazon.com; Path=/; Max-Age=0");
    let foreign = HeaderValue::from_static("other=1; Domain=.example.com; Path=/");
    carried.set_cookies(&mut [&removed, &foreign].into_iter(), &url);
    let set = carried.set.lock().unwrap();
    let kept: Vec<(&str, bool)> = set
        .iter()
        .map(|replied| (replied.cookie.name.as_str(), replied.removed))
        .collect();
    assert_eq!(kept, [("session-token", false), ("x-old", true)]);
    assert_eq!(set[0].cookie.value, "three");
}
