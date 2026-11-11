//! When a live read opens the DSP's browser rather than reading natively: as the ladder answers
//! each read, its native reader failing or not.
use super::*;

/// A browser read the day and handed back a native reader.
fn browsed(ladder: &mut Ladder) {
    ladder.browsing();
    ladder.browsed();
}

#[test]
fn cortex_briefly_unavailable_fails_a_few_reads_in_a_row_before_one_opens_the_browser() {
    let mut ladder = Ladder::default();
    browsed(&mut ladder);
    ladder.read_natively();
    for _ in 0..2 {
        // Each read fails with no browser, until the third in a row opens it.
        for _ in 0..UNAVAILABLE_READS {
            assert!(ladder.unavailable());
        }
        assert!(!ladder.unavailable());
        browsed(&mut ladder);
        // A whole read starts the count again.
        assert!(ladder.unavailable());
        ladder.read_natively();
    }
    // Unavailable is no failure of the reader: the next read goes native.
    assert!(ladder.native_first());
}

#[test]
fn a_reader_failing_fresh_from_a_browser_leaves_the_next_reads_to_the_browser() {
    let mut ladder = Ladder::default();
    // One that read since it came fails: only that read opens the browser.
    browsed(&mut ladder);
    ladder.read_natively();
    ladder.failed();
    browsed(&mut ladder);
    assert!(ladder.native_first());
    // The one that browser handed back fails at once: that read opens the browser, as do the
    // next with no native read first, until native is tried again.
    ladder.failed();
    browsed(&mut ladder);
    let mut browser_reads = 0;
    while !ladder.native_first() {
        browsed(&mut ladder);
        browser_reads += 1;
    }
    assert_eq!(browser_reads, BROWSER_READS - 1);
    // And again, while a fresh one keeps failing.
    ladder.failed();
    browsed(&mut ladder);
    assert!(!ladder.native_first());
}

use dispatch_core::collection::browser::http::{FIXTURE, SignedIn};
use std::{future::Future, task::Poll};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn authorized() -> dispatch_core::collection::browser::Pending<'static, ()> {
    Box::pin(async { Ok(()) })
}

fn request() -> Request {
    Request {
        collection: routes::Collection::Routes,
        mode: routes::Mode::Final,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Logistics".into(),
        dsp_abbreviation: "FXTR".into(),
        service_area_id: Some("area-1".into()),
        provider: Some("company-1".into()),
    }
}

fn response(status: u16, delay: Option<u64>, body: &str) -> String {
    let retry = delay
        .map(|d| format!("Retry-After: {d}\r\n"))
        .unwrap_or_default();
    format!(
        "HTTP/1.1 {status} Fixture\r\n{retry}Content-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn native(responses: Vec<String>) -> (Native, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    let server = tokio::spawn(async move {
        for response in responses {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                headers.push(stream.read_u8().await.unwrap());
            }
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let native = Native::new(&SignedIn {
        origin,
        cookies: Vec::new(),
        user_agent: "Fixture".into(),
    })
    .unwrap();
    (native, server)
}

#[tokio::test]
async fn throttling_and_cancelling_its_cooldown_keep_the_reader_until_its_connection_changes() {
    let (native, server) = native(
        [0, 0, 0, 20]
            .into_iter()
            .map(|delay| response(429, Some(delay), ""))
            .collect(),
    )
    .await;
    let request = request();
    let mut held = Held {
        native: Some(Handed {
            native,
            scope: routes::fixture(&request).unwrap().scope,
            revision: 7,
        }),
        ..Held::default()
    };
    for _ in 0..4 {
        let result = held.read_native(&request, Some(7), &authorized).await;
        assert_eq!(
            result.err().map(|e| e.code).as_deref(),
            Some("cortex_throttled")
        );
        assert!(held.native.is_some());
        assert_eq!(held.ladder, Ladder::default());
    }
    server.await.unwrap();
    let mut pending = Box::pin(held.read_native(&request, Some(7), &authorized));
    std::future::poll_fn(|cx| {
        assert!(pending.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(pending);
    assert!(held.native.is_some());
    assert!(
        held.read_native(&request, Some(8), &authorized)
            .await
            .unwrap()
            .is_err()
    );
    assert!(held.native.is_none());
}

#[tokio::test]
async fn a_first_read_keeps_a_long_cooldown_and_releases_subsequent_turns_without_waiting() {
    let (native, server) = native(vec![response(429, Some(3600), "")]).await;
    let request = request();
    let scope = routes::fixture(&request).unwrap().scope;
    let mut held = Held::default();
    held.ladder.browser_reads = 3;
    let first = held
        .read_fresh(native, scope, 7, &request, &authorized)
        .await;
    assert_eq!(
        first.err().map(|e| e.code).as_deref(),
        Some("cortex_throttled")
    );
    server.await.unwrap();
    assert!(held.native.as_ref().unwrap().native.has_cooldown());
    let mut next = Box::pin(held.read_native(&request, Some(7), &authorized));
    std::future::poll_fn(|cx| {
        // No timer or provider request may occupy the shared turn during a long cooldown.
        let Poll::Ready(Err(error)) = next.as_mut().poll(cx) else {
            panic!("long cooldown held the turn or lost its reader")
        };
        assert_eq!(error.code, "cortex_throttled");
        Poll::Ready(())
    })
    .await;
    drop(next);
    assert!(held.native.is_some());
}

#[tokio::test]
async fn cancelling_a_first_native_read_retains_its_cooldown_before_the_browser_handoff() {
    let page = r#"<title>DSP Console</title>{"hmacSession":{"sessionKey":"fixture-key","sessionId":"fixture-session"}}"#;
    let (native, server) =
        native(vec![response(200, None, page), response(429, Some(20), "")]).await;
    let request = request();
    let scope = routes::fixture(&request).unwrap().scope;
    let mut held = Held::default();
    held.ladder.browser_reads = 3;
    let mut reading = Box::pin(held.read_fresh(native, scope, 7, &request, &authorized));
    // Cancel after the first throttle was observed, while the first native read awaits
    // its retry. Polls are driven by the provider future's own wakeups.
    std::future::poll_fn(|cx| {
        let (result, events) = observability::recorded(|| reading.as_mut().poll(cx));
        assert!(result.is_pending());
        if events
            .iter()
            .any(|e| e["event"] == "cortex.routes_read_failed" && e["fields"]["status"] == 429)
        {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    drop(reading);
    server.await.unwrap();
    assert!(held.native.as_ref().unwrap().native.has_cooldown());
    let mut next = Box::pin(held.read_native(&request, Some(7), &authorized));
    std::future::poll_fn(|cx| {
        // The browser ladder cannot discard a newly handed reader's outstanding cooldown.
        assert!(next.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(next);
    assert!(held.native.is_some());
    assert!(
        held.read_native(&request, Some(8), &authorized)
            .await
            .unwrap()
            .is_err()
    );
    assert!(held.native.is_none());
}
