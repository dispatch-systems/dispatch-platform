//! How Dispatch reads a website's client document: from the public internet and nowhere
//! else. Anyone can name any URL as their client id, so the fetch is the attacker's request
//! made from inside Dispatch's network. The host is resolved here and every address it has
//! must be public; the request then goes to the address that was checked and no other, never
//! resolving the host again, over HTTPS, without following a redirect, within 5 seconds and
//! 64 KiB. Tests and fixture mode put another [`Network`] in place of the internet, so they
//! never touch it.
use crate::{Error, Result, browsers::egress, ensure, observability};
use serde_json::json;
use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    pin::Pin,
    time::Duration,
};
use url::Url;

/// Longest a website's client id may be.
const LONGEST: usize = 512;
const PORT: u16 = 443;
const TIMEOUT: Duration = Duration::from_secs(5);

pub type Pending<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where a website's client document comes from.
pub trait Network: Send + Sync {
    /// Every address `host` resolves to.
    fn resolve<'a>(&'a self, host: &'a str) -> Pending<'a, Result<Vec<IpAddr>>>;
    /// `GET url` from `address` alone: its host is never looked up again and a redirect is
    /// never followed. Answers the status and, for a 200, the body as far as `limit` bytes
    /// and one more, so a longer one shows as too long without being read to its end.
    fn get<'a>(
        &'a self,
        url: &'a Url,
        address: SocketAddr,
        limit: usize,
    ) -> Pending<'a, Result<(u16, Vec<u8>)>>;
}

fn unavailable() -> Error {
    Error::new("app_unavailable", 502)
}

/// The public internet, through the operating system's resolver.
pub struct Internet;
impl Network for Internet {
    fn resolve<'a>(&'a self, host: &'a str) -> Pending<'a, Result<Vec<IpAddr>>> {
        Box::pin(async move {
            let found = tokio::net::lookup_host((host, PORT))
                .await
                .map_err(|_| unavailable())?;
            Ok(found.map(|address| address.ip()).collect())
        })
    }
    fn get<'a>(
        &'a self,
        url: &'a Url,
        address: SocketAddr,
        limit: usize,
    ) -> Pending<'a, Result<(u16, Vec<u8>)>> {
        Box::pin(async move {
            let failed = |_| unavailable();
            let host = url.host_str().ok_or_else(unavailable)?;
            // The host resolves to the checked address and nothing else, and no proxy
            // stands between: a proxy would resolve the host again itself.
            let client = reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .no_proxy()
                .resolve(host, address)
                .connect_timeout(TIMEOUT)
                .timeout(TIMEOUT)
                .user_agent("Dispatch")
                .build()
                .map_err(failed)?;
            let mut response = client
                .get(url.clone())
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
                .map_err(failed)?;
            let status = response.status().as_u16();
            let mut body = Vec::new();
            if status == 200 {
                while let Some(chunk) = response.chunk().await.map_err(failed)? {
                    body.extend_from_slice(&chunk);
                    if body.len() > limit {
                        break;
                    }
                }
            }
            Ok((status, body))
        })
    }
}

/// The network in fixture mode: one sample website, and no other host resolves. Nothing is
/// ever connected to.
pub struct Fixture;
/// The sample website's client id, for fixture mode and tests.
pub const FIXTURE_APP: &str = "https://app.dispatch.test/oauth/client.json";
const FIXTURE_HOST: &str = "app.dispatch.test";
impl Network for Fixture {
    fn resolve<'a>(&'a self, host: &'a str) -> Pending<'a, Result<Vec<IpAddr>>> {
        Box::pin(async move {
            ensure(host == FIXTURE_HOST, "app_unavailable", 502)?;
            Ok(vec![IpAddr::V4(Ipv4Addr::new(93, 184, 215, 14))])
        })
    }
    fn get<'a>(
        &'a self,
        url: &'a Url,
        _: SocketAddr,
        _: usize,
    ) -> Pending<'a, Result<(u16, Vec<u8>)>> {
        Box::pin(async move {
            if url.as_str() != FIXTURE_APP {
                return Ok((404, Vec::new()));
            }
            let document = json!({
                "client_id": FIXTURE_APP,
                "client_name": "Example web app",
                "redirect_uris": ["https://app.dispatch.test/oauth/callback"],
            });
            Ok((200, document.to_string().into_bytes()))
        })
    }
}

/// A website's client id as Dispatch takes one: an `https` URL with a path, written exactly
/// as the URL standard writes it, naming its host by a dotted name on the standard port, with
/// no credentials, query or fragment, of at most 512 characters. None for anything else.
pub fn web_client_id(client_id: &str) -> Option<Url> {
    if client_id.len() > LONGEST {
        return None;
    }
    let url = Url::parse(client_id).ok()?;
    let named = matches!(url.host(), Some(url::Host::Domain(host)) if host.contains('.'));
    (url.scheme() == "https"
        && url.as_str() == client_id
        && named
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() != "/")
        .then_some(url)
}

/// Whether an address is on the public internet. Refused: private, loopback, link-local
/// (where cloud metadata services answer), unique-local, shared (CGNAT, 100.64/10),
/// multicast, documentation, benchmarking and every other reserved range, IPv4 written as
/// IPv6, and IPv6 outside global unicast.
pub fn public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(_) => egress::public_address(address),
        IpAddr::V6(ip) => public_v6(ip),
    }
}
fn public_v6(ip: Ipv6Addr) -> bool {
    let [first, second, ..] = ip.segments();
    // Global unicast is 2000::/3; loopback, unique-local, link-local, multicast, IPv4-mapped
    // and NAT64 addresses all lie outside it.
    first & 0xe000 == 0x2000
        // IETF protocol assignments, Teredo among them.
        && !(first == 0x2001 && second < 0x0200)
        // Documentation.
        && !(first == 0x2001 && second == 0x0db8)
        && !(first == 0x3fff && second < 0x1000)
        // 6to4, which carries any IPv4 address inside it.
        && first != 0x2002
        // Segment routing.
        && first != 0x5f00
}

/// A website's client document, fetched as this module says: within 5 seconds in all, from
/// a public address only, answered with 200 and at most `largest` bytes.
pub async fn fetch(network: &dyn Network, url: &Url, largest: usize) -> Result<Vec<u8>> {
    let fetched = async {
        let host = url.host_str().ok_or_else(unavailable)?;
        let addresses = network.resolve(host).await?;
        // Every address must be public: a host that also answers with a private one is
        // refused, whichever one a connection would have taken.
        let address = match addresses[..] {
            [first, ..] if addresses.iter().all(|address| public(*address)) => first,
            _ => {
                observability::event(
                    "warn",
                    "oauth.client_address_refused",
                    json!({"clientId":url.as_str()}),
                );
                return Err(unavailable());
            }
        };
        let (status, body) = network
            .get(url, SocketAddr::new(address, PORT), largest)
            .await?;
        // A redirect is never followed: anything but 200 is no document.
        ensure(status == 200, "app_unavailable", 502)?;
        ensure(body.len() <= largest, "app_unavailable", 502)?;
        Ok(body)
    };
    tokio::time::timeout(TIMEOUT, fetched)
        .await
        .map_err(|_| unavailable())?
}

#[cfg(test)]
#[path = "../../tests/backend/oauth/network.rs"]
mod tests;
