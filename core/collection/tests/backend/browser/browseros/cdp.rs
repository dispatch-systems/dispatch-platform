use super::*;
#[tokio::test]
async fn navigation_poll_is_fragment_safe_and_keeps_tabs_and_roster_events_separate() -> Result<()>
{
    let (client, server) = UnixStream::pair()?;
    server.set_nonblocking(true)?;
    let mut server = tokio::net::UnixStream::from_std(server)?;
    let mut cdp = Cdp::new(client)?;
    server
        .write_all(
            b"{\"method\":\"Fetch.requestPaused\",\"sessionId\":\"one\",\
            \"params\":{\"requestId\":\"roster\"}}\0{\"method\":\"Page.frameNavigated\",\
            \"sessionId\":\"two\",\
            \"params\":{\"frame\":{\"loaderId\":\"other\"}}}\0{\"method\":\"Page.frameNavigated\",",
        )
        .await?;
    assert!(cdp.navigation("one", "old").await?.is_null());
    server.write_all(
        b"\"sessionId\":\"one\",\"params\":{\"frame\":{\"loaderId\":\"new\",\"url\":\"https://fixture.invalid/card\"}}}\0").await?;

    assert_eq!(cdp.navigation("one", "old").await?["loaderId"], "new");
    assert_eq!(cdp.navigation("two", "old").await?["loaderId"], "other");
    assert_eq!(cdp.event("one").await?["requestId"], "roster");
    cdp.observe(&json!({"method":"Page.frameNavigated","sessionId":"one",
        "params":{"frame":{"parentId":"main","loaderId":"subframe"}}}))?;
    assert_eq!(cdp.frames["one"]["loaderId"], "new");
    cdp.observe(&json!({"method":"Target.detachedFromTarget","params":{"sessionId":"one"}}))?;
    assert!(!cdp.frames.contains_key("one"));
    for index in 0..7 {
        cdp.observe(
            &json!({"method":"Page.frameNavigated","sessionId":format!("page-{index}"),
            "params":{"frame":{"loaderId":"new"}}}),
        )?;
    }
    assert!(
        cdp.observe(
            &json!({"method":"Page.frameNavigated","sessionId":"overflow",
        "params":{"frame":{"loaderId":"new"}}})
        )
        .is_err()
    );
    Ok(())
}
#[tokio::test]
async fn event_poll_preserves_a_fragment_across_timeout() -> Result<()> {
    let (client, server) = UnixStream::pair()?;
    server.set_nonblocking(true)?;
    let mut server = tokio::net::UnixStream::from_std(server)?;
    server
        .write_all(b"{\"method\":\"Fetch.requestPaused\",")
        .await?;
    let mut cdp = Cdp::new(client)?;
    assert_eq!(cdp.event("page-1").await?, Value::Null);
    server
        .write_all(b"\"sessionId\":\"page-1\",\"params\":{\"requestId\":\"roster\"}}\0")
        .await?;
    assert_eq!(cdp.event("page-1").await?["requestId"], "roster");
    Ok(())
}
#[tokio::test]
async fn fetch_capture_is_bounded_and_scoped_to_its_page() -> Result<()> {
    let (client, _server) = UnixStream::pair()?;
    let mut cdp = Cdp::new(client)?;
    cdp.retain(json!({"sessionId":"other","params":{"requestId":"other"}}))?;
    cdp.retain(json!({"sessionId":"page-1","params":{"requestId":"roster"}}))?;
    assert_eq!(cdp.event("page-1").await?["requestId"], "roster");
    for _ in 1..16 {
        cdp.retain(json!({"sessionId":"other"}))?;
    }
    assert!(cdp.retain(json!({"sessionId":"other"})).is_err());
    Ok(())
}
#[tokio::test]
async fn transport_handles_fragmented_events_and_response() -> Result<()> {
    let (client, server) = UnixStream::pair()?;
    server.set_nonblocking(true)?;
    let mut server = BufReader::new(tokio::net::UnixStream::from_std(server)?);
    let task = tokio::spawn(async move {
        let mut request = Vec::new();
        server.read_until(0, &mut request).await?;
        let value: Value = serde_json::from_slice(&request[..request.len() - 1])?;
        require(value["sessionId"] == "page-1", "Session not forwarded")?;
        server
            .get_mut()
            .write_all(b"{\"method\":\"Page.loadEventFired\"}\0{\"id\":1,")
            .await?;
        server
            .get_mut()
            .write_all(b"\"result\":{\"ok\":true}}\0")
            .await?;
        Ok::<_, Error>(())
    });
    let mut cdp = Cdp::new(client)?;
    assert_eq!(
        cdp.command("Page.enable", json!({}), Some("page-1"))
            .await?,
        json!({"ok":true})
    );
    task.await.unwrap()?;
    Ok(())
}
#[tokio::test]
async fn transport_rejects_eof_and_oversized_frames() -> Result<()> {
    for bytes in [Vec::new(), vec![b' '; MAX_FRAME as usize]] {
        let (client, server) = UnixStream::pair()?;
        server.set_nonblocking(true)?;
        let mut server = BufReader::new(tokio::net::UnixStream::from_std(server)?);
        let task = tokio::spawn(async move {
            let mut request = Vec::new();
            let _ = server.read_until(0, &mut request).await;
            let _ = server.get_mut().write_all(&bytes).await;
        });
        assert!(
            Cdp::new(client)?
                .command("Browser.getVersion", json!({}), None)
                .await
                .is_err()
        );
        task.await.unwrap();
    }
    Ok(())
}
