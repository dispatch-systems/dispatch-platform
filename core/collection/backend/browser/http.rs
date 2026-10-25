//! A provider over plain HTTP from this process, with the session a browser signed
//! in. Signing in stays in the browser; only a collection's repeated reads come here.
//! Requests keep to the browser's egress rules: the provider's own HTTPS hosts on
//! public IPv4 addresses, no redirects or ambient proxies, bounded in size and time. An
//! isolated support verification uses its fixed trusted provider broker. Session cookies
//! stay in memory, for the job or a reader that goes on after it, and are never written or
//! logged.
use super::{browseros, egress};
use crate::{Error, Result, db::s};
use reqwest::{
    Client, Response, StatusCode,
    cookie::Jar,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{CONTENT_ENCODING, CONTENT_TYPE},
    redirect::Policy,
};
use serde_json::{Map, Value, json};
use std::{io::Read, net::SocketAddr, sync::Arc, time::Duration};
use url::Url;

/// As much as a reader in the tab accepts.
const LIMIT: usize = 2 * 1024 * 1024;
/// The local stand-in's host.
pub const FIXTURE: &str = "fixture.dispatch.invalid";

/// The hosts a provider's own requests go to, declared by its collector. They are
/// reached only over HTTPS on port 443, at public IPv4 addresses.
pub struct RequestHosts {
    pub allowed: fn(&str) -> bool,
    /// The domains whose browser cookies its requests carry.
    pub cookies: fn(&str) -> bool,
    /// Over HTTP/2 with compression; otherwise uncompressed HTTP/1.1.
    pub http2: bool,
}
impl RequestHosts {
    pub fn allows(&self, url: &Url) -> bool {
        url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.scheme() == "https"
            && url.port_or_known_default() == Some(443)
            && (self.allowed)(url.host_str().unwrap_or(""))
    }
}

/// Where requests may go: one provider's hosts, or the local stand-in the browser
/// was pointed at.
#[derive(Clone, Copy)]
enum Hosts {
    Provider(&'static RequestHosts),
    Fixture(u16),
}
impl Hosts {
    /// Where a session at `origin` may go: the local stand-in it names, or `hosts` when the
    /// origin is one of theirs.
    fn of(origin: &str, hosts: &'static RequestHosts) -> Result<Self> {
        let url = Url::parse(origin).map_err(|_| Error::new("egress_denied", 403))?;
        match url.host_str() {
            Some(FIXTURE) => Ok(Self::Fixture(
                url.port().ok_or_else(|| Error::new("egress_denied", 403))?,
            )),
            Some(host) if (hosts.allowed)(host) => Ok(Self::Provider(hosts)),
            _ => Err(Error::new("egress_denied", 403)),
        }
    }
    fn allows(self, url: &Url) -> bool {
        match self {
            Self::Provider(hosts) => hosts.allows(url),
            Self::Fixture(port) => {
                url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none()
                    && url.scheme() == "http"
                    && url.host_str() == Some(FIXTURE)
                    && url.port() == Some(port)
            }
        }
    }
    fn cookie(self, domain: &str) -> bool {
        match self {
            Self::Provider(hosts) => (hosts.cookies)(domain),
            Self::Fixture(_) => domain == FIXTURE,
        }
    }
    fn http2(self) -> bool {
        matches!(self, Self::Provider(hosts) if hosts.http2)
    }
}
/// Resolves only allowed hosts, and only to public IPv4 addresses.
struct Resolver(Hosts);
impl Resolve for Resolver {
    fn resolve(&self, name: Name) -> Resolving {
        let hosts = self.0;
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let addresses: Vec<SocketAddr> = match hosts {
                Hosts::Fixture(port) if host == FIXTURE => {
                    vec![SocketAddr::from(([127, 0, 0, 1], port))]
                }
                Hosts::Provider(provider) if (provider.allowed)(&host) => {
                    tokio::net::lookup_host((host, 443))
                        .await?
                        .filter(|a| a.is_ipv4() && egress::public_address(a.ip()))
                        .collect()
                }
                _ => Vec::new(),
            };
            if addresses.is_empty() {
                return Err("egress_denied".into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

/// Why a response was not used.
pub enum Refusal {
    /// Throttled, failing or unreachable: the same as a tab that could not load it.
    Unavailable,
    /// Anything else, named by a fixed label.
    Unreadable(&'static str),
}

/// A provider's session as a signed-in browser held it: the cookies its requests carry and
/// the browser's user agent, so a reader can go on with it after the browser closes. Kept in
/// memory only.
#[derive(Clone)]
pub struct SignedIn {
    /// The provider's origin: one of its hosts, or the local stand-in.
    pub origin: String,
    pub cookies: Vec<Cookie>,
    pub user_agent: String,
}
/// One cookie of a session, as the browser held it.
#[derive(Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
}
impl SignedIn {
    /// What `browser` holds for the provider at `origin`, one of its `hosts`: its cookies
    /// on the hosts' cookie domains, and its user agent.
    pub async fn of(
        browser: &browseros::Session,
        origin: &str,
        hosts: &'static RequestHosts,
    ) -> Result<Self> {
        let carried = Hosts::of(origin, hosts)?;
        let cookies = browser
            .command("Storage.getCookies", json!({}), None)
            .await?;
        let version = browser
            .command("Browser.getVersion", json!({}), None)
            .await?;
        let cookies = cookies["cookies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cookie| carried.cookie(s(cookie, "domain").trim_start_matches('.')))
            .map(|cookie| Cookie {
                name: s(cookie, "name").into(),
                value: s(cookie, "value").into(),
                domain: s(cookie, "domain").into(),
                path: s(cookie, "path").into(),
                secure: cookie["secure"] == true,
            })
            .collect();
        Ok(Self {
            origin: origin.trim_end_matches('/').to_owned(),
            cookies,
            user_agent: s(&version, "userAgent").into(),
        })
    }
}

/// A body as the wire sent it, from `Http::wire`.
pub struct Wire {
    body: Vec<u8>,
    gzipped: bool,
}
impl Wire {
    /// Its text within `limit` bytes, decoded when it came gzipped, as strict as `json_text`.
    /// Decoding takes a core: a reader on the threads serving requests hands it to a blocking
    /// one.
    pub fn into_text(self, limit: usize) -> std::result::Result<String, Refusal> {
        let bytes = if self.gzipped {
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(self.body.as_slice())
                .take(limit as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| Refusal::Unreadable("http_encoding"))?;
            decoded
        } else {
            self.body
        };
        if bytes.len() > limit {
            return Err(Refusal::Unreadable("http_too_large"));
        }
        utf8(bytes)
    }
}

pub struct Http {
    client: Client,
    hosts: Hosts,
    origin: String,
    /// How many of the browser's cookies its requests carry.
    cookies: usize,
}
pub struct Download {
    pub body: Option<Vec<u8>>,
    pub etag: Option<String>,
    pub modified_at: Option<i64>,
}
impl Http {
    /// A client that carries nothing of a browser's session: no cookies, user agent or
    /// referer. It reaches only `hosts`, or the local stand-in when `origin` is one.
    pub fn detached(origin: &str, hosts: &'static RequestHosts) -> Result<Self> {
        let url = Url::parse(origin).map_err(|_| Error::new("egress_denied", 403))?;
        let hosts = if url.host_str() == Some(FIXTURE) {
            Hosts::Fixture(url.port().ok_or_else(|| Error::new("egress_denied", 403))?)
        } else {
            Hosts::Provider(hosts)
        };
        let mut client = verification_proxy(Client::builder().redirect(Policy::none()))?;
        if !hosts.http2() {
            client = client.http1_only().no_gzip();
        }
        let client = client
            .dns_resolver(Arc::new(Resolver(hosts)))
            .https_only(!matches!(hosts, Hosts::Fixture(_)))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        Ok(Self {
            client,
            hosts,
            origin: origin.into(),
            cookies: 0,
        })
    }
    pub async fn download(
        &self,
        url: &str,
        etag: Option<&str>,
        limit: usize,
    ) -> std::result::Result<Download, Refusal> {
        let mut request = self
            .client
            .get(self.target(url)?)
            .timeout(Duration::from_secs(60));
        if let Some(etag) = etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let mut response = request.send().await.map_err(|_| Refusal::Unavailable)?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(Refusal::Unavailable);
        }
        if status == StatusCode::FORBIDDEN {
            return Err(Refusal::Unreadable("http_download_expired"));
        }
        if status != StatusCode::OK && status != StatusCode::NOT_MODIFIED {
            return Err(Refusal::Unreadable("http_download_refused"));
        }
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|h| h.to_str().ok())
                .map(str::to_owned)
        };
        let etag = header(reqwest::header::ETAG).filter(|e| e.len() <= 256);
        let modified_at = header(reqwest::header::LAST_MODIFIED)
            .and_then(|s| chrono::DateTime::parse_from_rfc2822(&s).ok())
            .map(|t| t.timestamp_millis());
        if status == StatusCode::NOT_MODIFIED {
            return Ok(Download {
                body: None,
                etag,
                modified_at,
            });
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(Refusal::Unreadable("http_body_too_large"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Refusal::Unavailable)? {
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(Refusal::Unreadable("http_body_too_large"));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Download {
            body: Some(body),
            etag,
            modified_at,
        })
    }
    /// The browser's session for the provider at `origin`, one of its `hosts`: its
    /// cookies and user agent, nothing else.
    pub async fn signed_in(
        browser: &browseros::Session,
        origin: &str,
        hosts: &'static RequestHosts,
    ) -> Result<Self> {
        Self::carrying(&SignedIn::of(browser, origin, hosts).await?, hosts, true)
    }
    /// A client carrying `session` as `signed_in` makes one, but handing bodies back as the
    /// wire sent them, for a reader that asks for gzip with `wire` and decodes them itself.
    pub fn with_session(session: &SignedIn, hosts: &'static RequestHosts) -> Result<Self> {
        Self::carrying(session, hosts, false)
    }
    fn carrying(session: &SignedIn, hosts: &'static RequestHosts, decoded: bool) -> Result<Self> {
        let hosts = Hosts::of(&session.origin, hosts)?;
        let scheme = if matches!(hosts, Hosts::Fixture(_)) {
            "http"
        } else {
            "https"
        };
        let jar = Jar::default();
        let mut carried = 0;
        for cookie in &session.cookies {
            let host = cookie.domain.trim_start_matches('.');
            if !hosts.cookie(host) {
                continue;
            }
            let Ok(url) = Url::parse(&format!("{scheme}://{host}/")) else {
                continue;
            };
            let mut line = format!("{}={}; Path={}", cookie.name, cookie.value, cookie.path);
            if cookie.domain.starts_with('.') {
                line.push_str(&format!("; Domain={host}"));
            }
            if cookie.secure {
                line.push_str("; Secure");
            }
            jar.add_cookie_str(&line, &url);
            carried += 1;
        }
        let mut client = verification_proxy(
            Client::builder()
                .cookie_provider(Arc::new(jar))
                .redirect(Policy::none()),
        )?;
        // HTTP/2 only where the provider's hosts ask for it, never for the local stand-in.
        if !hosts.http2() {
            client = client.http1_only();
        }
        // Compressed only over HTTP/2, and decoded here unless the reader decodes itself.
        if !hosts.http2() || !decoded {
            client = client.no_gzip();
        }
        let client = client
            .dns_resolver(Arc::new(Resolver(hosts)))
            .https_only(!matches!(hosts, Hosts::Fixture(_)))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(&session.user_agent)
            .build()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        Ok(Self {
            client,
            hosts,
            origin: session.origin.clone(),
            cookies: carried,
        })
    }
    /// Whether the browser held any cookie of the provider's for its requests: one that
    /// never signed in, or whose session was cleared, holds none.
    pub fn carries_cookies(&self) -> bool {
        self.cookies > 0
    }
    fn target(&self, value: &str) -> std::result::Result<Url, Refusal> {
        Url::parse(value)
            .ok()
            .filter(|url| self.hosts.allows(url))
            .ok_or(Refusal::Unreadable("http_egress_denied"))
    }
    /// A body of the expected type, within the size a tab would accept.
    async fn body(response: Response, kind: &str) -> std::result::Result<String, Refusal> {
        Self::body_within(response, kind, LIMIT).await
    }
    /// A body of the expected type, within `limit` bytes.
    async fn body_within(
        response: Response,
        kind: &str,
        limit: usize,
    ) -> std::result::Result<String, Refusal> {
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(Refusal::Unavailable);
        }
        if status.is_redirection()
            || [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&status)
        {
            return Err(Refusal::Unreadable("http_signed_out"));
        }
        if status != StatusCode::OK {
            return Err(Refusal::Unreadable("http_status"));
        }
        utf8(Self::bytes(response, kind, limit).await?)
    }
    /// An answer's bytes as they came, when it is of type `kind` and within `limit`.
    async fn bytes(
        mut response: Response,
        kind: &str,
        limit: usize,
    ) -> std::result::Result<Vec<u8>, Refusal> {
        let expected = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|v| v.trim().eq_ignore_ascii_case(kind));
        if !expected {
            return Err(Refusal::Unreadable("http_content_type"));
        }
        if response
            .content_length()
            .is_some_and(|length| length > limit as u64)
        {
            return Err(Refusal::Unreadable("http_too_large"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Refusal::Unavailable)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > limit {
                return Err(Refusal::Unreadable("http_too_large"));
            }
        }
        Ok(bytes)
    }
    /// Sends `body` with `headers` as a page at the session's origin would, and answers
    /// the JSON text that comes back, within the size a tab accepts. A target outside the
    /// provider's hosts fails `egress_denied`; no usable answer, `provider_unavailable`.
    pub async fn post(
        &self,
        url: &str,
        headers: &Map<String, Value>,
        body: String,
        timeout: Duration,
    ) -> Result<String> {
        let target = self
            .target(url)
            .map_err(|_| Error::new("egress_denied", 403))?;
        let mut request = self
            .client
            .post(target)
            .timeout(timeout)
            .header("Origin", &self.origin)
            .header("Referer", format!("{}/", self.origin))
            .body(body);
        for (key, value) in headers {
            request = request.header(key, value.as_str().unwrap_or(""));
        }
        let response = request
            .send()
            .await
            .map_err(|_| Error::new("provider_unavailable", 502))?;
        Self::body(response, "application/json")
            .await
            .map_err(|_| Error::new("provider_unavailable", 502))
    }
    /// A JSON document within `limit` bytes, as a tab's `fetch` of it would receive.
    pub async fn json(
        &self,
        url: &str,
        referer: &str,
        limit: usize,
    ) -> std::result::Result<Value, Refusal> {
        let text = self.json_text(url, &[], referer, limit).await?;
        serde_json::from_str(&text).map_err(|_| Refusal::Unreadable("http_not_json"))
    }
    /// A JSON document's text within `limit` bytes, unparsed, sent with `headers` as well:
    /// those a page signed this exact address with, which its own request would carry.
    pub async fn json_text(
        &self,
        url: &str,
        headers: &[(String, String)],
        referer: &str,
        limit: usize,
    ) -> std::result::Result<String, Refusal> {
        let target = self.target(url)?;
        let mut request = self
            .client
            .get(target)
            .timeout(Duration::from_secs(60))
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", referer);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let response = request.send().await.map_err(|_| Refusal::Unavailable)?;
        Self::body_within(response, "application/json", limit).await
    }
    /// A GET of `url`, sent with `headers` as well, that asks for gzip and answers the body
    /// as the wire sent it, within `limit` bytes as sent. `kind` is the type it must be,
    /// `application/json` or `text/html`, asked for as a tab asks. Unlike the reads above,
    /// it refuses a redirect as `http_redirected` and a 401 or 403 as `http_refused`, so a
    /// reader signing its own requests tells a session that ended from a signature refused.
    pub async fn wire(
        &self,
        url: &str,
        kind: &str,
        headers: &[(String, String)],
        referer: &str,
        limit: usize,
    ) -> std::result::Result<Wire, Refusal> {
        let target = self.target(url)?;
        let accept = if kind == "text/html" {
            "text/html,application/xhtml+xml"
        } else {
            "application/json, text/plain, */*"
        };
        let mut request = self
            .client
            .get(target)
            .timeout(Duration::from_secs(60))
            .header("Accept", accept)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", referer);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let response = request.send().await.map_err(|_| Refusal::Unavailable)?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(Refusal::Unavailable);
        }
        if status.is_redirection() {
            return Err(Refusal::Unreadable("http_redirected"));
        }
        if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&status) {
            return Err(Refusal::Unreadable("http_refused"));
        }
        if status != StatusCode::OK {
            return Err(Refusal::Unreadable("http_status"));
        }
        let gzipped = match response
            .headers()
            .get(CONTENT_ENCODING)
            .map(|v| v.to_str().map(str::trim))
        {
            None => false,
            Some(Ok(encoding)) if encoding.eq_ignore_ascii_case("identity") => false,
            Some(Ok(encoding)) if encoding.eq_ignore_ascii_case("gzip") => true,
            Some(_) => return Err(Refusal::Unreadable("http_encoding")),
        };
        Ok(Wire {
            body: Self::bytes(response, kind, limit).await?,
            gzipped,
        })
    }
    /// Where `url` redirects, as a tab following it would go next: for an address the
    /// provider answers with the place a file is served from. The redirect itself is not
    /// followed; its target is the caller's to check.
    pub async fn location(&self, url: &str, referer: &str) -> std::result::Result<Url, Refusal> {
        let target = self.target(url)?;
        let response = self
            .client
            .get(target.clone())
            .timeout(Duration::from_secs(30))
            .header("Accept", "image/avif,image/webp,image/*,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", referer)
            .send()
            .await
            .map_err(|_| Refusal::Unavailable)?;
        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return Err(Refusal::Unavailable);
        }
        if [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN].contains(&status) {
            return Err(Refusal::Unreadable("http_signed_out"));
        }
        if !status.is_redirection() {
            return Err(Refusal::Unreadable("http_not_redirected"));
        }
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .filter(|value| value.len() <= 4096)
            .and_then(|value| target.join(value).ok())
            .ok_or(Refusal::Unreadable("http_location_invalid"))
    }
    /// One timecard page's HTML, as a tab's `fetch` of it would receive.
    pub async fn page(&self, url: &str, referer: &str) -> std::result::Result<String, Refusal> {
        let target = self.target(url)?;
        let response = self
            .client
            .get(target)
            .timeout(Duration::from_secs(30))
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", referer)
            .send()
            .await
            .map_err(|_| Refusal::Unavailable)?;
        Self::body(response, "text/html").await
    }
}

/// Text as `TextDecoder('utf-8', {fatal: true})` reads it: strict, a leading BOM dropped.
fn utf8(bytes: Vec<u8>) -> std::result::Result<String, Refusal> {
    let text = String::from_utf8(bytes).map_err(|_| Refusal::Unreadable("http_encoding"))?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

fn verification_proxy(client: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder> {
    if std::env::var_os("SUPPORT_VERIFICATION_EGRESS").is_some() {
        crate::ensure(
            std::env::var("SUPPORT_VERIFICATION_EGRESS").as_deref()
                == Ok("/run/support-provider/egress.sock")
                && std::env::var("DISPATCH_ENVIRONMENT").as_deref() == Ok("preview")
                && std::env::var("NODE_ENV").as_deref() == Ok("development"),
            "egress_denied",
            403,
        )?;
        Ok(client.no_proxy().proxy(
            reqwest::Proxy::https("http://127.0.0.1:17892")
                .map_err(|_| Error::new("egress_denied", 403))?,
        ))
    } else {
        Ok(client.no_proxy())
    }
}
