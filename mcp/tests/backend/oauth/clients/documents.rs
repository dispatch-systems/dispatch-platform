use super::super::{KNOWN, KnownApp};
use super::{
    Documents, FRESH, Fetched, KEPT, MOST_WEBSITES, RETRY, WEBSITE_REQUESTS, Website, document,
    known_document,
};
use crate::oauth::network::FIXTURE_APP;
use dispatch_core::{Error, Result, db::now};
use serde_json::json;
use std::{future::Future, time::Duration};

#[test]
fn the_built_in_documents_are_the_known_apps_own() {
    for KnownApp {
        url, name, copy, ..
    } in KNOWN
    {
        let client = document(url, "Fallback", copy.as_bytes()).unwrap();
        assert_eq!((client.name.as_str(), client.known), (*name, true));
        assert!(!client.redirect_uris.is_empty());
    }
    // A document naming another client, or none of its redirects, is no document.
    let KnownApp { url, copy, .. } = KNOWN[0];
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
    let KnownApp { copy, .. } = KNOWN[1];
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
        document.fetched.lock().unwrap().insert(KNOWN[2].url, entry);
    };
    set(
        &other,
        Fetched {
            document: None,
            failed: Some(now() - RETRY - 1),
        },
    );
    let KnownApp {
        url, name, copy, ..
    } = KNOWN[2];
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

    // A request that finds every place among the website requests taken is refused before it
    // touches the cache, which gains a URL's entry before any fetch for it; once a place is
    // free, the same request is answered.
    crate::testing::install();
    let (_root, db) = dispatch_core::testing::store();
    let admitting = Documents::default();
    let permits: Vec<_> = (0..WEBSITE_REQUESTS)
        .map(|_| admitting.website_requests.try_acquire().unwrap())
        .collect();
    assert_eq!(
        admitting
            .website(&db.config, FIXTURE_APP)
            .await
            .unwrap_err(),
        "app_unavailable"
    );
    assert!(admitting.websites.lock().unwrap().is_empty());
    drop(permits);
    let answered = admitting.website(&db.config, FIXTURE_APP).await.unwrap();
    assert_eq!(answered.id, FIXTURE_APP);

    let documents = Documents::default();
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
