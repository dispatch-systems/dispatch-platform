//! Redirect header contracts of the production physical-attempt helper.
use super::*;

fn follow(head: Request, location: &[u8]) -> Option<Request> {
    let response = Response::from(
        axum::http::Response::builder()
            .status(307)
            .header(
                header::LOCATION,
                header::HeaderValue::from_bytes(location).unwrap(),
            )
            .body("")
            .unwrap(),
    );
    let replay = head.try_clone();
    redirected(&response, head, replay, Policy::Ordinary, 0).unwrap()
}

#[test]
fn a_generated_referer_does_not_survive_a_later_https_to_http_hop() {
    let first = Request::new(
        Method::GET,
        Url::parse("https://files.example.test/private?file=one#part").unwrap(),
    );
    let second = follow(first, b"https://cdn.example.test/second").unwrap();
    assert_eq!(
        second.headers().get(header::REFERER).unwrap(),
        "https://files.example.test/private?file=one"
    );
    let third = follow(second, b"http://cdn.example.test/final").unwrap();
    assert!(!third.headers().contains_key(header::REFERER));
}

#[test]
fn locations_accept_valid_utf8_and_decline_invalid_utf8() {
    let first = || {
        Request::new(
            Method::GET,
            Url::parse("https://files.example.test/first").unwrap(),
        )
    };
    let next = follow(first(), "/café".as_bytes()).expect("valid UTF-8 Location was ignored");
    assert_eq!(next.url().path(), "/caf%C3%A9");
    assert!(follow(first(), b"/bad\xff").is_none());
}
