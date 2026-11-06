//! Published client documents and their bounded, shared fetch cache.
use super::super::network::{self, Network};
use super::{
    Client, KNOWN, KnownApp,
    redirects::{allowed, loopback},
};
use dispatch_core::{
    Error, Result,
    db::now,
    ensure,
    foundation::{config::Config, observability},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

/// A document is fetched again after an hour, and while that fails the last one serves a day.
/// A failed fetch is not tried again for a minute.
const FRESH: i64 = 60 * 60 * 1000;
const KEPT: i64 = 24 * 60 * 60 * 1000;
const RETRY: i64 = 60 * 1000;
const LARGEST: usize = 64 * 1024;
/// Websites' documents kept at once, and fetched at once.
const MOST_WEBSITES: usize = 256;
/// Every request waiting for or performing a website-document fetch. Active network work is
/// bounded more tightly below; this cap also bounds per-URL mutex and semaphore waiters.
const WEBSITE_REQUESTS: usize = 32;
const WEBSITE_FETCHES: usize = 4;

type Found = std::result::Result<Client, &'static str>;

/// The known apps' documents as last fetched, and when a fetch last failed. One fetch per
/// app runs at a time; the requests that wait for it take its answer. Websites' documents
/// are kept the same way, by URL, a few hundred at most.
pub struct Documents {
    fetched: Mutex<HashMap<&'static str, Fetched>>,
    fetching: [tokio::sync::Mutex<()>; KNOWN.len()],
    websites: Mutex<HashMap<String, Website>>,
    website_requests: tokio::sync::Semaphore,
    website_fetches: tokio::sync::Semaphore,
    /// Where websites' documents come from, when not the internet or fixture mode's.
    network: Mutex<Option<Arc<dyn Network>>>,
}
impl Default for Documents {
    fn default() -> Self {
        Self {
            fetched: Mutex::default(),
            fetching: Default::default(),
            websites: Mutex::default(),
            website_requests: tokio::sync::Semaphore::new(WEBSITE_REQUESTS),
            website_fetches: tokio::sync::Semaphore::new(WEBSITE_FETCHES),
            network: Mutex::default(),
        }
    }
}
/// A website's document as last fetched, the fetch under way for it, and when it was last
/// asked for, so the longest unused goes first when too many are kept.
#[derive(Default)]
struct Website {
    fetched: Fetched,
    fetching: Arc<tokio::sync::Mutex<()>>,
    asked: i64,
}
#[derive(Clone, Default)]
struct Fetched {
    document: Option<(i64, Client)>,
    failed: Option<i64>,
}
impl Fetched {
    /// What an app's document answers without fetching it, when that is settled: a fresh
    /// document, or after a recent failure the last document while it is kept.
    fn settled(&self) -> Option<Found> {
        if let Some((at, client)) = &self.document
            && now() - at < FRESH
        {
            return Some(Ok(client.clone()));
        }
        self.failed
            .is_some_and(|at| now() - at < RETRY)
            .then(|| self.kept())
    }
    fn kept(&self) -> Found {
        self.document
            .as_ref()
            .filter(|(at, _)| now() - at < KEPT)
            .map(|(_, client)| client.clone())
            .ok_or("app_unavailable")
    }
}
impl Documents {
    /// The known app whose client document is at `url`, or the error page's code:
    /// `unknown_app` for any other URL, `app_unavailable` when its document cannot be had.
    pub async fn client(&self, config: &Config, url: &str) -> Found {
        let Some(index) = KNOWN.iter().position(|known| known.url == url) else {
            return Err("unknown_app");
        };
        let KnownApp {
            url: known,
            name,
            copy,
            ..
        } = KNOWN[index];
        if config.fixture {
            return document(known, name, copy.as_bytes()).ok_or("app_unavailable");
        }
        self.resolve(index, || fetch(known)).await
    }

    async fn resolve<F: Future<Output = Result<Vec<u8>>>>(
        &self,
        index: usize,
        fetch: impl FnOnce() -> F,
    ) -> Found {
        let KnownApp {
            url: known,
            name,
            copy,
            ..
        } = KNOWN[index];
        let entry = || {
            self.fetched
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .get(known)
                .cloned()
                .unwrap_or_default()
        };
        let keep = |entry| {
            self.fetched
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(known, entry);
        };
        // A fetched known document may remove redirects, but it may not silently widen the
        // reviewed embedded redirect boundary. A legitimate addition is picked up only after
        // the embedded copy changes in a reviewed release.
        let parse = |body: &[u8]| known_document(known, name, copy.as_bytes(), body);
        settle(known, &self.fetching[index], entry, keep, parse, fetch).await
    }

    /// A website's or other app's document, at any `https` URL on the public internet, read
    /// as the [`network`] module says. Only for an app the owner lets connect: such an app
    /// is never known. `unknown_app` for a URL no website's client id can be.
    pub async fn website(&self, config: &Config, url: &str) -> Found {
        let Some(target) = network::web_client_id(url) else {
            return Err("unknown_app");
        };
        // Do not queue unbounded HTTP requests behind the active-fetch semaphore or one URL's
        // mutex. A caller that cannot reserve its complete wait returns before cache mutation.
        let _request = self
            .website_requests
            .try_acquire()
            .map_err(|_| "app_unavailable")?;
        let network = self.network(config);
        self.resolve_website(url, || async move {
            let _slot = self
                .website_fetches
                .acquire()
                .await
                .map_err(|_| Error::new("app_unavailable", 502))?;
            network::fetch(network.as_ref(), &target, LARGEST).await
        })
        .await
    }

    async fn resolve_website<F: Future<Output = Result<Vec<u8>>>>(
        &self,
        url: &str,
        fetch: impl FnOnce() -> F,
    ) -> Found {
        let websites = || {
            self.websites
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
        };
        let fetching = {
            let mut websites = websites();
            if !websites.contains_key(url) && websites.len() >= MOST_WEBSITES {
                // The longest unused that no request is fetching makes room.
                let unused = websites
                    .iter()
                    .filter(|(_, website)| Arc::strong_count(&website.fetching) == 1)
                    .min_by_key(|(_, website)| website.asked)
                    .map(|(url, _)| url.clone());
                let Some(unused) = unused else {
                    return Err("app_unavailable");
                };
                websites.remove(&unused);
            }
            let website = websites.entry(url.to_owned()).or_default();
            website.asked = now();
            website.fetching.clone()
        };
        let entry = || {
            websites()
                .get(url)
                .map(|website| website.fetched.clone())
                .unwrap_or_default()
        };
        let keep = |fetched| {
            if let Some(website) = websites().get_mut(url) {
                website.fetched = fetched;
            }
        };
        settle(
            url,
            &fetching,
            entry,
            keep,
            |body| website_document(url, body),
            fetch,
        )
        .await
    }

    /// Tests read websites' documents from `network` instead of the internet.
    pub fn use_network(&self, network: Arc<dyn Network>) {
        *self
            .network
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(network);
    }
    fn network(&self, config: &Config) -> Arc<dyn Network> {
        let chosen = self
            .network
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        chosen.unwrap_or_else(|| {
            if config.fixture {
                Arc::new(network::Fixture)
            } else {
                Arc::new(network::Internet)
            }
        })
    }
}

/// An app's document as `entry` holds it, or fetched under `fetching` when that is not
/// settled, and given to `keep`. Requests that waited for a fetch take its answer.
async fn settle<F: Future<Output = Result<Vec<u8>>>>(
    url: &str,
    fetching: &tokio::sync::Mutex<()>,
    entry: impl Fn() -> Fetched,
    keep: impl FnOnce(Fetched),
    parse: impl FnOnce(&[u8]) -> Option<Client>,
    fetch: impl FnOnce() -> F,
) -> Found {
    if let Some(found) = entry().settled() {
        return found;
    }
    let _only = fetching.lock().await;
    // Whoever fetched while this request waited has settled it.
    let mut entry = entry();
    if let Some(found) = entry.settled() {
        return found;
    }
    let found = match fetch().await.ok().and_then(|body| parse(&body)) {
        Some(client) => {
            entry.document = Some((now(), client.clone()));
            entry.failed = None;
            Ok(client)
        }
        None => {
            observability::event(
                "warn",
                "oauth.client_document_failed",
                json!({"clientId":url}),
            );
            entry.failed = Some(now());
            entry.kept()
        }
    };
    keep(entry);
    found
}

/// A client document's body: HTTPS only, no redirects, within 5 seconds and 64 KiB.
async fn fetch(url: &str) -> Result<Vec<u8>> {
    let failed = |_| Error::new("app_unavailable", 502);
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .user_agent("Dispatch")
        .build()
        .map_err(failed)?;
    let mut response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(failed)?;
    ensure(response.status() == 200, "app_unavailable", 502)?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        body.extend_from_slice(&chunk);
        ensure(body.len() <= LARGEST, "app_unavailable", 502)?;
    }
    Ok(body)
}

/// A client document as Dispatch uses it: its `client_id` must be the URL it came from and it
/// must list where the app may be sent back to. Only its `https` and loopback redirects
/// count; any other is ignored. How the app says it authenticates is ignored too, since only
/// public clients exist here.
fn document(url: &str, fallback: &str, body: &[u8]) -> Option<Client> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value["client_id"].as_str() != Some(url) {
        return None;
    }
    let redirect_uris: Vec<String> = value["redirect_uris"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|uri| {
            loopback(uri).is_some()
                || url::Url::parse(uri)
                    .is_ok_and(|url| url.scheme() == "https" && url.fragment().is_none())
        })
        .map(str::to_owned)
        .collect();
    if redirect_uris.is_empty() {
        return None;
    }
    Some(Client {
        id: url.to_owned(),
        name: value["client_name"]
            .as_str()
            .and_then(crate::names::shown)
            .unwrap_or_else(|| fallback.to_owned()),
        known: true,
        redirect_uris,
    })
}
/// A fetched known document constrained to the redirect boundary reviewed in `embedded`.
fn known_document(url: &str, fallback: &str, embedded: &[u8], fetched: &[u8]) -> Option<Client> {
    let baseline = document(url, fallback, embedded)?;
    let client = document(url, fallback, fetched)?;
    client
        .redirect_uris
        .iter()
        .all(|redirect| allowed(&baseline.redirect_uris, redirect))
        .then_some(client)
}
/// A website's or other app's client document: read as a known app's is, but only ever
/// app-provided metadata, by the name it gives itself or else its host.
fn website_document(url: &str, body: &[u8]) -> Option<Client> {
    let host = url::Url::parse(url).ok()?.host_str()?.to_owned();
    Some(Client {
        known: false,
        ..document(url, &host, body)?
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/oauth/clients/documents.rs"]
mod tests;
