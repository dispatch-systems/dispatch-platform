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
        (
            403,
            Some("appNotAuthorizedToFile"),
            "documents_item_not_found",
        ),
        (404, None, "documents_item_not_found"),
        (403, Some("rateLimitExceeded"), "google_unreachable"),
    ] {
        assert_eq!(refusal(status, error).code, code, "{status} {error:?}");
    }
}

#[test]
fn a_refusal_is_logged_in_googles_words_without_the_ids_and_addresses_in_them() {
    assert_eq!(
        unnamed(
            "The user has not granted the app 123 write access to the file 1AbCdEfGhIjKlMnOpQrStUvWxYz0123."
        ),
        "The user has not granted the app 123 write access to the file {id}"
    );
    assert_eq!(
        unnamed("owner@example.com does not have permission."),
        "{email} does not have permission."
    );
}

/// A local HTTP/2 peer returns actual protocol NACKs/redirects to the real transport. It
/// cannot contact Google; both the listener and its request URL are loopback addresses.
#[tokio::test]
async fn redirect_hops_and_protocol_retries_admit_the_original_owner_for_each_attempt() {
    use crate::backend::{
        access, grant,
        storage::DocumentsStore,
        support::{connect_as, platform},
    };
    use transport::Policy;
    for mode in [
        "redirect",
        "redirect_utf8",
        "retry",
        "retry_limit",
        "picture",
    ] {
        for removed in [false, true] {
            let (_root, state, dsp) = platform();
            let account = format!(
                "fixture:transport-{}@example.test",
                crypto::token().unwrap().to_ascii_lowercase()
            );
            connect_as(&state, &dsp, &account).await;
            let owner = grant::capture(&state, &dsp, None).await.unwrap();
            let drive = access::open_for(&state, &owner, false).await.unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (entered, waiting) = tokio::sync::oneshot::channel();
            let (release, released) = tokio::sync::oneshot::channel::<()>();
            let peer =
                tokio::spawn(async move {
                    let (socket, _) = listener.accept().await.unwrap();
                    let mut connection = h2::server::handshake(socket).await.unwrap();
                    let mut requests = Vec::new();
                    let mut entered = Some(entered);
                    let mut released = Some(released);
                    while let Some(request) = connection.accept().await {
                        let (request, mut response) = request.unwrap();
                        requests.push((
                            request.uri().path().to_owned(),
                            request
                                .headers()
                                .get(reqwest::header::AUTHORIZATION)
                                .cloned(),
                        ));
                        if let Some(entered) = entered.take() {
                            entered.send(()).unwrap();
                            released.take().unwrap().await.unwrap();
                        }
                        if mode == "retry_limit" || (mode == "retry" && requests.len() <= 2) {
                            response.send_reset(h2::Reason::REFUSED_STREAM);
                        } else {
                            let redirected =
                                matches!(mode, "redirect" | "redirect_utf8" | "picture")
                                    && requests.len() == 1;
                            let mut answer = axum::http::Response::builder()
                                .status(if redirected { 307 } else { 200 });
                            if redirected {
                                let location = if mode == "redirect_utf8" {
                                    reqwest::header::HeaderValue::from_bytes("/café".as_bytes())
                                        .unwrap()
                                } else {
                                    reqwest::header::HeaderValue::from_static("/next")
                                };
                                answer = answer.header("location", location);
                            }
                            response
                                .send_response(answer.body(()).unwrap(), true)
                                .unwrap();
                        }
                    }
                    requests
                });
            let client = reqwest::Client::builder()
                .no_proxy()
                .http2_prior_knowledge()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap();
            let request = client
                .get(format!("http://{address}/first"))
                .bearer_auth("fixture-token");
            let work = tokio::spawn(async move {
                drive
                    .google
                    .response(
                        request,
                        if mode == "picture" {
                            Policy::Picture
                        } else {
                            Policy::Ordinary
                        },
                    )
                    .await
                    .map(|response| response.status().as_u16())
            });
            waiting.await.unwrap();
            if removed {
                state
                    .run(move |db| db.remove_documents_connection(&dsp))
                    .await
                    .unwrap();
            }
            release.send(()).unwrap();
            let answer = work.await.unwrap();
            drop(client);
            let requests = peer.await.unwrap();
            if mode == "picture" {
                // The picture policy stops at a non-Google Location, even with a current owner.
                assert_eq!(answer.unwrap(), 307);
                assert_eq!(requests.len(), 1);
            } else if removed {
                assert_eq!(answer.unwrap_err().code, "documents_not_connected");
                assert_eq!(
                    requests.len(),
                    1,
                    "{mode}: stale owner sent another physical request"
                );
            } else {
                let expected = if matches!(mode, "redirect" | "redirect_utf8") {
                    2
                } else {
                    3
                };
                assert_eq!(requests.len(), expected, "{mode}");
                if mode == "retry_limit" {
                    assert_eq!(answer.unwrap_err().code, "google_unreachable");
                } else {
                    assert_eq!(answer.unwrap(), 200);
                }
                assert_eq!(
                    requests.last().unwrap().0,
                    if mode == "redirect_utf8" {
                        "/caf%C3%A9"
                    } else if mode == "redirect" {
                        "/next"
                    } else {
                        "/first"
                    }
                );
            }
            assert!(
                requests
                    .iter()
                    .all(|(_, auth)| auth.as_ref().unwrap() == "Bearer fixture-token")
            );
        }
    }
}

#[tokio::test]
async fn a_followed_cross_origin_redirect_strips_credentials_and_keeps_the_method_and_body() {
    use crate::backend::{
        access, grant,
        support::{connect_as, platform},
    };
    use axum::{
        body::Bytes,
        http::{HeaderMap, Method},
    };

    async fn serve(
        listener: tokio::net::TcpListener,
        location: Option<String>,
    ) -> (Method, HeaderMap, Vec<u8>) {
        let (socket, _) = listener.accept().await.unwrap();
        let mut connection = h2::server::handshake(socket).await.unwrap();
        let mut handled = None;
        while let Some(request) = connection.accept().await {
            assert!(handled.is_none(), "only one request may reach each origin");
            let (mut request, mut response) = request.unwrap();
            let location = location.clone();
            handled =
                Some(tokio::spawn(async move {
                    let mut body = Vec::new();
                    while let Some(chunk) = request.body_mut().data().await {
                        body.extend_from_slice(&chunk.unwrap());
                    }
                    let mut answer = axum::http::Response::builder()
                        .status(if location.is_some() { 307 } else { 200 });
                    if let Some(location) = location {
                        answer = answer.header("location", location);
                    }
                    response
                        .send_response(answer.body(()).unwrap(), true)
                        .unwrap();
                    (request.method().clone(), request.headers().clone(), body)
                }));
        }
        handled.unwrap().await.unwrap()
    }
    let (_root, state, dsp) = platform();
    let account = format!(
        "fixture:origins-{}@example.test",
        crypto::token().unwrap().to_ascii_lowercase()
    );
    connect_as(&state, &dsp, &account).await;
    let owner = grant::capture(&state, &dsp, None).await.unwrap();
    let drive = access::open_for(&state, &owner, false).await.unwrap();
    let source = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/first", source.local_addr().unwrap());
    let destination = format!("http://{}/next", target.local_addr().unwrap());
    let source_peer = tokio::spawn(serve(source, Some(destination)));
    let target_peer = tokio::spawn(serve(target, None));
    let client = reqwest::Client::builder()
        .no_proxy()
        .http2_prior_knowledge()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap();
    let request = client
        .post(&origin)
        .bearer_auth("fixture-token")
        .header("cookie", "fixture-cookie")
        .header("cookie2", "fixture-cookie2")
        .header("proxy-authorization", "fixture-proxy")
        .header("www-authenticate", "fixture-auth")
        .header("content-type", "application/octet-stream")
        .body(Bytes::from_static(b"replayable"));
    assert_eq!(
        drive
            .google
            .response(request, transport::Policy::Ordinary)
            .await
            .unwrap()
            .status(),
        200
    );
    drop(client);
    let (method, headers, body) = source_peer.await.unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(body, b"replayable");
    assert!(headers.contains_key("authorization"));
    let (method, headers, body) = target_peer.await.unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(body, b"replayable");
    assert_eq!(headers["content-type"], "application/octet-stream");
    for name in [
        "authorization",
        "cookie",
        "cookie2",
        "proxy-authorization",
        "www-authenticate",
    ] {
        assert!(!headers.contains_key(name), "{name} leaked across origins");
    }
    assert_eq!(headers["referer"], origin);
}
