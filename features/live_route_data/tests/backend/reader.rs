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

#[tokio::test]
async fn throttling_and_cancelling_its_cooldown_keep_the_reader_until_its_connection_changes() {
    use dispatch_core::collection::browser::http::{FIXTURE, SignedIn};
    use std::{future::Future, task::Poll};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{FIXTURE}:{}", listener.local_addr().unwrap().port());
    let server = tokio::spawn(async move {
        // Several throttled slots must not escalate to the browser. The last response
        // asks for a cooldown in which the next read can be cancelled.
        for delay in [0, 0, 0, 120] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                headers.push(stream.read_u8().await.unwrap());
            }
            stream.write_all(format!("HTTP/1.1 429 Too Many Requests\r\nRetry-After: {delay}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        }
    });
    let request = Request {
        collection: routes::Collection::Routes,
        mode: routes::Mode::Final,
        date: "2026-09-25".into(),
        station: "TST1".into(),
        timezone: "America/Los_Angeles".into(),
        dsp_name: "Fixture Logistics".into(),
        dsp_abbreviation: "FXTR".into(),
        service_area_id: Some("area-1".into()),
        provider: Some("company-1".into()),
    };
    let mut held = Held {
        native: Some(Handed {
            native: Native::new(&SignedIn {
                origin,
                cookies: Vec::new(),
                user_agent: "Fixture".into(),
            })
            .unwrap(),
            scope: routes::fixture(&request).unwrap().scope,
            revision: 7,
        }),
        ..Held::default()
    };
    for _ in 0..4 {
        let result = held.read_native(&request, Some(7)).await;
        assert_eq!(
            result.err().map(|e| e.code).as_deref(),
            Some("cortex_throttled")
        );
        assert!(held.native.is_some());
        assert_eq!(held.ladder, Ladder::default());
    }
    server.await.unwrap();
    let mut pending = Box::pin(held.read_native(&request, Some(7)));
    std::future::poll_fn(|cx| {
        assert!(pending.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(pending);
    assert!(held.native.is_some());
    // A changed connection still ends the held session, even during a cooldown.
    assert!(held.read_native(&request, Some(8)).await.unwrap().is_err());
    assert!(held.native.is_none());
}
