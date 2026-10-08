//! How Documents reads Google's refusals: only Google giving up on the token breaks the
//! connection, so an outage never sends an owner to reconnect.
use super::*;

#[test]
fn only_a_token_google_gave_up_on_breaks_the_connection() {
    for (status, error, code) in [
        (400, Some("invalid_grant"), "google_connection_broken"),
        (401, None, "google_connection_broken"),
        (429, None, "google_unreachable"),
        (503, None, "google_unreachable"),
        (400, Some("invalid_request"), "google_sign_in_failed"),
    ] {
        assert_eq!(refusal(status, error).code, code, "{status} {error:?}");
    }
}
