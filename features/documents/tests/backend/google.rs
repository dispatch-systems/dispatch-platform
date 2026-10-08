//! How Documents reads Google's refusals: only Google giving up on the token breaks the
//! connection, so an outage never sends an owner to reconnect, and a full account or a file
//! Dispatch can't reach says so.
use super::*;

#[test]
fn only_a_token_google_gave_up_on_breaks_the_connection_and_drive_says_why_else() {
    for (status, error, code) in [
        (400, Some("invalid_grant"), "google_connection_broken"),
        (401, None, "google_connection_broken"),
        (429, None, "google_unreachable"),
        (503, None, "google_unreachable"),
        (400, Some("invalid_request"), "google_sign_in_failed"),
        // Drive's own: a full account, and a file that is gone or was never Dispatch's.
        (403, Some("storageQuotaExceeded"), "documents_storage_full"),
        (403, Some("appNotAuthorizedToFile"), "documents_item_not_found"),
        (404, None, "documents_item_not_found"),
        (403, Some("rateLimitExceeded"), "google_unreachable"),
    ] {
        assert_eq!(refusal(status, error).code, code, "{status} {error:?}");
    }
}
