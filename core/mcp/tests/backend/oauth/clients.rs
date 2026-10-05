use super::*;

#[test]
fn the_built_in_documents_are_the_known_apps_own() {
    for (url, name, copy) in KNOWN {
        let client = document(url, "Fallback", copy.as_bytes()).unwrap();
        assert_eq!((client.name.as_str(), client.known), (*name, true));
        assert!(!client.redirect_uris.is_empty());
    }
    // A document naming another client, or none of its redirects, is no document.
    let (url, _, copy) = KNOWN[0];
    assert!(document("https://chatgpt.com/oauth/other.json", "x", copy.as_bytes()).is_none());
    let bare = json!({"client_id":url,"redirect_uris":[]}).to_string();
    assert!(document(url, "x", bare.as_bytes()).is_none());
    let unnamed = json!({"client_id":url,"redirect_uris":["https://a.example/cb"]});
    let client = document(url, "ChatGPT", unnamed.to_string().as_bytes()).unwrap();
    assert_eq!(client.name, "ChatGPT");
    assert!(document(url, "x", b"<html>").is_none());
    // Only https and loopback redirects count; a document listing none is no document.
    let mixed = json!({"client_id":url,"redirect_uris":[
        "javascript:alert(1)", "http://evil.example/cb", "myapp.example://cb",
        "https://a.example/cb", "https://a.example/cb#x", "http://127.0.0.1/cb", 7]});
    let client = document(url, "x", mixed.to_string().as_bytes()).unwrap();
    assert_eq!(
        client.redirect_uris,
        ["https://a.example/cb", "http://127.0.0.1/cb"]
    );
    let none = json!({"client_id":url,"redirect_uris":["cursor://x/cb"]});
    assert!(document(url, "x", none.to_string().as_bytes()).is_none());
    let widened = json!({"client_id":url,"redirect_uris":["https://evil.example/cb"]});
    assert!(
        known_document(
            url,
            "ChatGPT",
            copy.as_bytes(),
            widened.to_string().as_bytes()
        )
        .is_none()
    );
}

fn fetched(body: &str) -> impl Future<Output = Result<Vec<u8>>> {
    let body = body.to_owned();
    async move { Ok(body.into_bytes()) }
}
async fn failed() -> Result<Vec<u8>> {
    Err(Error::new("app_unavailable", 502))
}

#[tokio::test]
async fn documents_are_fetched_once_at_a_time_and_failures_wait_a_minute() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (_, _, copy) = KNOWN[1];
    let documents = Documents::default();
    let fetches = AtomicUsize::new(0);
    // Requests arriving together share one fetch.
    let slow = || {
        fetches.fetch_add(1, Ordering::SeqCst);
        async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(copy.as_bytes().to_vec())
        }
    };
    let (a, b, c) = tokio::join!(
        documents.resolve(1, slow),
        documents.resolve(1, slow),
        documents.resolve(1, slow)
    );
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    assert!(a.is_ok() && b.is_ok() && c.is_ok());
    // Fresh for an hour: no fetch at all.
    let found = documents.resolve(1, failed).await.unwrap();
    assert_eq!(found.name, "Codex");
    // A failed fetch is not tried again for a minute, and nothing is fetched meanwhile.
    let other = Documents::default();
    assert_eq!(
        other.resolve(2, failed).await.unwrap_err(),
        "app_unavailable"
    );
    let tried = AtomicUsize::new(0);
    let counted = || {
        tried.fetch_add(1, Ordering::SeqCst);
        fetched(copy)
    };
    assert_eq!(
        other.resolve(2, counted).await.unwrap_err(),
        "app_unavailable"
    );
    assert_eq!(tried.load(Ordering::SeqCst), 0);
    // After the minute it is fetched again.
    let set = |document: &Documents, entry: Fetched| {
        document.fetched.lock().unwrap().insert(KNOWN[2].0, entry);
    };
    set(
        &other,
        Fetched {
            document: None,
            failed: Some(now() - RETRY - 1),
        },
    );
    let (url, name, copy) = KNOWN[2];
    assert_eq!(other.resolve(2, || fetched(copy)).await.unwrap().id, url);
    // A stale document that cannot be fetched again still serves its day.
    let stale = document(url, name, copy.as_bytes()).unwrap();
    set(
        &other,
        Fetched {
            document: Some((now() - FRESH - 1, stale.clone())),
            failed: None,
        },
    );
    assert_eq!(other.resolve(2, failed).await.unwrap().id, url);
    set(
        &other,
        Fetched {
            document: Some((now() - KEPT - 1, stale)),
            failed: None,
        },
    );
    assert_eq!(
        other.resolve(2, failed).await.unwrap_err(),
        "app_unavailable"
    );
}

#[tokio::test]
async fn website_document_waiters_and_cache_are_hard_bounded() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let documents = Documents::default();
    let permits: Vec<_> = (0..WEBSITE_REQUESTS)
        .map(|_| documents.website_requests.try_acquire().unwrap())
        .collect();
    assert!(documents.website_requests.try_acquire().is_err());
    drop(permits);

    // If every cached URL has a request holding its fetch mutex, a new URL is refused
    // instead of making the nominal cache cap best-effort.
    let mut held = Vec::with_capacity(MOST_WEBSITES);
    {
        let mut websites = documents.websites.lock().unwrap();
        for index in 0..MOST_WEBSITES {
            let website = Website::default();
            held.push(website.fetching.clone());
            websites.insert(format!("https://{index}.example/client.json"), website);
        }
    }
    let fetched = AtomicBool::new(false);
    assert_eq!(
        documents
            .resolve_website("https://overflow.example/client.json", || {
                fetched.store(true, Ordering::SeqCst);
                failed()
            })
            .await
            .unwrap_err(),
        "app_unavailable"
    );
    assert!(!fetched.load(Ordering::SeqCst));
    assert_eq!(documents.websites.lock().unwrap().len(), MOST_WEBSITES);

    // Once one entry is no longer in use, it can make room, but the map never grows.
    held.pop();
    assert_eq!(
        documents
            .resolve_website("https://replacement.example/client.json", failed)
            .await
            .unwrap_err(),
        "app_unavailable"
    );
    let websites = documents.websites.lock().unwrap();
    assert_eq!(websites.len(), MOST_WEBSITES);
    assert!(websites.contains_key("https://replacement.example/client.json"));
}

#[test]
fn loopback_redirects_match_on_any_port_and_nothing_else_does() {
    let registered = |uris: &[&str]| uris.iter().map(|u| (*u).to_owned()).collect::<Vec<_>>();
    let portless = registered(&["http://localhost/callback", "http://127.0.0.1/callback"]);
    assert!(allowed(&portless, "http://localhost:53122/callback"));
    assert!(allowed(&portless, "http://127.0.0.1:61001/callback"));
    assert!(allowed(&portless, "http://127.0.0.1/callback"));
    for wrong in [
        "http://127.0.0.1:61001/callback2",
        "http://127.0.0.1:61001/callback?x=1",
        "https://127.0.0.1:61001/callback",
        "http://127.0.0.2:61001/callback",
        "http://127.0.0.1:99999/callback",
        "http://127.0.0.1:/callback",
        "http://localhost.evil.example/callback",
        "http://localhost@evil.example/callback",
    ] {
        assert!(!allowed(&portless, wrong), "{wrong}");
    }
    // localhost and 127.0.0.1 are different hosts.
    assert!(!allowed(
        &registered(&["http://localhost/cb"]),
        "http://127.0.0.1:5/cb"
    ));
    assert!(allowed(
        &registered(&["http://[::1]:7/cb"]),
        "http://[::1]:9/cb"
    ));
    // Anything else matches only exactly, port and all.
    let web = registered(&["https://chatgpt.com/connector_platform_oauth_redirect"]);
    assert!(allowed(
        &web,
        "https://chatgpt.com/connector_platform_oauth_redirect"
    ));
    assert!(!allowed(
        &web,
        "https://chatgpt.com:443/connector_platform_oauth_redirect"
    ));
    assert!(!allowed(
        &web,
        "https://chatgpt.com/connector_platform_oauth_redirect/"
    ));
    let app = registered(&["cursor://anysphere.cursor-retrieval/oauth/callback"]);
    assert!(allowed(
        &app,
        "cursor://anysphere.cursor-retrieval/oauth/callback"
    ));
    assert!(!allowed(
        &app,
        "cursor://anysphere.cursor-retrieval:1/oauth/callback"
    ));
}

#[test]
fn apps_register_only_redirects_to_this_computer() {
    for uri in [
        "http://127.0.0.1:27890/callback",
        "http://localhost/callback",
        "http://[::1]:8080/cb?x=1",
        "cursor://anysphere.cursor-retrieval/oauth/callback",
        "vscode://vscode.github-authentication/did-authenticate",
        "vscode-insiders://callback",
        "windsurf://codeium.windsurf/callback",
        "com.example.app:/oauth2redirect",
    ] {
        assert!(registrable(uri), "{uri}");
    }
    // A scheme is a native app's only when reverse-domain or a known app's; a web+ scheme
    // is a website's protocol handler.
    for uri in [
        "web+dsp://cb",
        "web+dsp.example://cb",
        "magnet:?xt=urn:btih:abc",
        "myapp://callback",
        "sms:5550100",
    ] {
        assert!(private_scheme(uri).is_none(), "{uri}");
        assert!(!registrable(uri), "{uri}");
    }
    let long = format!("http://127.0.0.1:1/{}", "a".repeat(LONGEST_REDIRECT));
    assert!(!registrable(&long));
    for uri in [
        "https://evil.example/callback",
        "http://evil.example/callback",
        "http://127.0.0.1:27890",
        "http://127.0.0.1:27890/callback#here",
        "javascript:alert(1)",
        "data:text/html,hi",
        "file:///etc/passwd",
        "JavaScript:alert(1)",
        "Cursor://x/y",
        "1app://x",
        "cursor:",
        "cursor://x/ y",
    ] {
        assert!(!registrable(uri), "{uri}");
    }
    assert_eq!(
        destination("http://127.0.0.1:5/callback"),
        ("this computer".into(), None)
    );
    assert_eq!(
        destination("cursor://anysphere.cursor-retrieval/oauth/callback"),
        ("this computer".into(), Some("cursor".into()))
    );
    assert_eq!(
        destination("https://chatgpt.com/connector_platform_oauth_redirect"),
        ("chatgpt.com".into(), None)
    );
}

#[test]
fn names_are_trimmed_and_cut_short() {
    assert_eq!(display_name("  Codex\n "), Some("Codex".into()));
    assert_eq!(display_name(&"a".repeat(100)).unwrap().len(), 80);
    assert_eq!(display_name(" \u{7} "), None);
    // Invisible and reordering characters cannot hide or disguise a name.
    assert_eq!(
        display_name("\u{202E}edoC \u{200B}edualC\u{2066}\u{FEFF}"),
        Some("edoC edualC".into())
    );
    assert_eq!(display_name("\u{200F}\u{2069}\u{E0041}"), None);
}
