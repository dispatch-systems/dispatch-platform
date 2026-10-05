use super::*;

#[test]
fn the_request_cookie_is_the_sessions_kind_but_survives_the_trip_from_the_app() {
    assert_eq!(
        browser_cookie(false, "authreq_1", "n", BROWSER_SECONDS),
        "__Host-dispatch_oauth_request_authreq_1=n; Path=/; HttpOnly; SameSite=Lax; \
         Max-Age=600; Secure"
    );
    assert_eq!(
        browser_cookie(true, "authreq_1", "", 0),
        "dispatch_oauth_request_authreq_1=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
    );
}
