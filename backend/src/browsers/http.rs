//! A provider over plain HTTP from this process, with the session a browser signed
//! in. Signing in stays in the browser; only a collection's repeated reads come here.
//! Requests keep to the browser's egress rules: the provider's own HTTPS hosts on
//! public IPv4 addresses, no redirects and no proxies, bounded in size and time. The
//! session's cookies stay in memory for the job and are never written or logged.
use super::{browseros, egress};
use crate::{Error, Result, db::s, ensure};
use reqwest::{
    Client, Response, StatusCode,
    cookie::Jar,
    dns::{Addrs, Name, Resolve, Resolving},
    header::CONTENT_TYPE,
    redirect::Policy,
};
use serde_json::{Map, Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use url::Url;

/// As much as a reader in the tab accepts.
const LIMIT: usize = 2 * 1024 * 1024;
const FIXTURE: &str = "fixture.dispatch.invalid";

/// Where requests may go: one provider's hosts, or the local stand-in the browser
/// was pointed at.
#[derive(Clone, Copy)]
enum Hosts {
    Paycom,
    Cortex,
    CortexReports,
    Fixture(u16),
}
const CORTEX: &str = "logistics.amazon.com";
impl Hosts {
    fn allows(self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && match self {
                Self::Paycom => {
                    url.scheme() == "https"
                        && url.port_or_known_default() == Some(443)
                        && Self::paycom(host)
                }
                Self::Cortex => {
                    url.scheme() == "https"
                        && url.port_or_known_default() == Some(443)
                        && Self::cortex(host)
                }
                Self::CortexReports => {
                    url.scheme() == "https"
                        && url.port_or_known_default() == Some(443)
                        && host == crate::dvic::REPORT_HOST
                        && egress::allowed_cortex_host(host)
                }
                Self::Fixture(port) => {
                    url.scheme() == "http" && host == FIXTURE && url.port() == Some(port)
                }
            }
    }
    /// Whether the provider's own requests may go to `host`.
    fn host(self, host: &str) -> bool {
        match self {
            Self::Paycom => Self::paycom(host),
            Self::Cortex => Self::cortex(host),
            Self::CortexReports => {
                host == crate::dvic::REPORT_HOST && egress::allowed_cortex_host(host)
            }
            Self::Fixture(_) => host == FIXTURE,
        }
    }
    fn paycom(host: &str) -> bool {
        egress::allowed_host(host)
            && (host == "paycomonline.net" || host.ends_with(".paycomonline.net"))
    }
    // Only the application's own host: its data API lives there.
    fn cortex(host: &str) -> bool {
        egress::allowed_cortex_host(host) && host == CORTEX
    }
    fn cookie(self, domain: &str) -> bool {
        match self {
            Self::Paycom => Self::paycom(domain),
            Self::Cortex => {
                domain == CORTEX || domain == "amazon.com" || domain.ends_with(".amazon.com")
            }
            Self::CortexReports => false,
            Self::Fixture(_) => domain == FIXTURE,
        }
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
                Hosts::Paycom | Hosts::Cortex | Hosts::CortexReports if hosts.host(&host) => {
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
pub(super) enum Refusal {
    /// Throttled, failing or unreachable: the same as a tab that could not load it.
    Unavailable,
    /// Anything else, named by a fixed label.
    Unreadable(&'static str),
}

pub(super) struct Http {
    client: Client,
    hosts: Hosts,
    origin: String,
}
pub(super) struct Download {
    pub body: Option<Vec<u8>>,
    pub etag: Option<String>,
    pub modified_at: Option<i64>,
}
impl Http {
    /// S3 pre-signed requests use a separate client with no Amazon cookies or
    /// referer. Redirects and every host other than the observed report host fail.
    pub fn cortex_reports(origin: &str) -> Result<Self> {
        let url = Url::parse(origin).map_err(|_| Error::new("egress_denied", 403))?;
        let hosts = if url.host_str() == Some(FIXTURE) {
            Hosts::Fixture(url.port().ok_or_else(|| Error::new("egress_denied", 403))?)
        } else {
            ensure(
                url.origin().ascii_serialization() == format!("https://{CORTEX}"),
                "egress_denied",
                403,
            )?;
            Hosts::CortexReports
        };
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(Resolver(hosts)))
            .https_only(!matches!(hosts, Hosts::Fixture(_)))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        Ok(Self {
            client,
            hosts,
            origin: origin.into(),
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
    /// The browser's session for the provider at `origin`: its cookies and user
    /// agent, nothing else.
    pub async fn signed_in(browser: &browseros::Session, origin: &str) -> Result<Self> {
        let origin_url = Url::parse(origin).map_err(|_| Error::new("egress_denied", 403))?;
        let hosts = match origin_url.host_str() {
            Some(FIXTURE) => Hosts::Fixture(
                origin_url
                    .port()
                    .ok_or_else(|| Error::new("egress_denied", 403))?,
            ),
            Some(CORTEX) => Hosts::Cortex,
            Some(host) if Hosts::paycom(host) => Hosts::Paycom,
            _ => return Err(Error::new("egress_denied", 403)),
        };
        let cookies = browser
            .command("Storage.getCookies", json!({}), None)
            .await?;
        let version = browser
            .command("Browser.getVersion", json!({}), None)
            .await?;
        let jar = Jar::default();
        let scheme = origin_url.scheme();
        for cookie in cookies["cookies"].as_array().into_iter().flatten() {
            let domain = s(cookie, "domain");
            let host = domain.trim_start_matches('.');
            if !hosts.cookie(host) {
                continue;
            }
            let Ok(url) = Url::parse(&format!("{scheme}://{host}/")) else {
                continue;
            };
            let mut line = format!(
                "{}={}; Path={}",
                s(cookie, "name"),
                s(cookie, "value"),
                s(cookie, "path")
            );
            if domain.starts_with('.') {
                line.push_str(&format!("; Domain={host}"));
            }
            if cookie["secure"] == true {
                line.push_str("; Secure");
            }
            jar.add_cookie_str(&line, &url);
        }
        let client = Client::builder()
            .cookie_provider(Arc::new(jar))
            .redirect(Policy::none())
            .no_proxy()
            .dns_resolver(Arc::new(Resolver(hosts)))
            .https_only(!matches!(hosts, Hosts::Fixture(_)))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(s(&version, "userAgent"))
            .build()
            .map_err(|_| Error::new("browser_unavailable", 503))?;
        Ok(Self {
            client,
            hosts,
            origin: origin.trim_end_matches('/').to_owned(),
        })
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
        let mut response = response;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Refusal::Unavailable)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > limit {
                return Err(Refusal::Unreadable("http_too_large"));
            }
        }
        // As `TextDecoder('utf-8', {fatal: true})`: strict, with a leading BOM dropped.
        let text = String::from_utf8(bytes).map_err(|_| Refusal::Unreadable("http_encoding"))?;
        Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned())
    }
    /// The roster request the search page made, sent again with the selected period.
    pub async fn roster(
        &self,
        url: &str,
        headers: &Map<String, Value>,
        body: String,
    ) -> Result<Value> {
        let target = self
            .target(url)
            .map_err(|_| Error::new("egress_denied", 403))?;
        let mut request = self
            .client
            .post(target)
            .timeout(Duration::from_secs(55))
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
        let text = Self::body(response, "application/json")
            .await
            .map_err(|_| Error::new("provider_unavailable", 502))?;
        let value: Value =
            serde_json::from_str(&text).map_err(|_| Error::new("roster_not_complete", 409))?;
        ensure(value.is_object(), "roster_not_complete", 409)?;
        Ok(value)
    }
    /// A JSON document within `limit` bytes, as a tab's `fetch` of it would receive.
    pub async fn json(
        &self,
        url: &str,
        referer: &str,
        limit: usize,
    ) -> std::result::Result<Value, Refusal> {
        let target = self.target(url)?;
        let response = self
            .client
            .get(target)
            .timeout(Duration::from_secs(60))
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", referer)
            .send()
            .await
            .map_err(|_| Refusal::Unavailable)?;
        let text = Self::body_within(response, "application/json", limit).await?;
        serde_json::from_str(&text).map_err(|_| Refusal::Unreadable("http_not_json"))
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

#[cfg(test)]
mod dvic_tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Response},
        routing::get,
    };
    #[tokio::test]
    async fn report_downloads_use_validators_bound_bodies_and_never_follow_redirects_or_forward_cookies()
     {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Router::new()
            .route(
                "/report",
                get(|headers: HeaderMap| async move {
                    assert!(headers.get("cookie").is_none());
                    assert!(headers.get("referer").is_none());
                    let unchanged = headers.get("if-none-match").is_some_and(|h| h == "\"v1\"");
                    Response::builder()
                        .status(if unchanged { 304 } else { 200 })
                        .header("etag", "\"v1\"")
                        .header("last-modified", "Sun, 27 Sep 2026 14:04:00 GMT")
                        .body(Body::from(if unchanged { "" } else { "workbook" }))
                        .unwrap()
                }),
            )
            .route(
                "/redirect",
                get(|| async {
                    Response::builder()
                        .status(302)
                        .header("location", "/report")
                        .body(Body::empty())
                        .unwrap()
                }),
            )
            .route(
                "/throttled",
                get(|| async { StatusCode::TOO_MANY_REQUESTS }),
            );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let origin = format!("http://{FIXTURE}:{port}");
        let client = Http::cortex_reports(&origin).unwrap();
        let url = format!("{origin}/report");
        let first = client.download(&url, None, 8).await.ok().unwrap();
        assert_eq!(first.body.unwrap(), b"workbook");
        assert!(first.modified_at.is_some());
        let cached = client
            .download(&url, first.etag.as_deref(), 8)
            .await
            .ok()
            .unwrap();
        assert!(cached.body.is_none());
        assert!(matches!(
            client.download(&url, None, 4).await,
            Err(Refusal::Unreadable("http_body_too_large"))
        ));
        assert!(matches!(
            client
                .download(&format!("{origin}/redirect"), None, 8)
                .await,
            Err(Refusal::Unreadable("http_download_refused"))
        ));
        assert!(matches!(
            client
                .download(&format!("{origin}/throttled"), None, 8)
                .await,
            Err(Refusal::Unavailable)
        ));
        let s3 = Url::parse(&format!("https://{}/report", crate::dvic::REPORT_HOST)).unwrap();
        assert!(Hosts::CortexReports.allows(&s3));
        assert!(!Hosts::Cortex.allows(&s3));
        assert!(!Hosts::CortexReports.cookie("amazon.com"));
        assert!(
            !Hosts::CortexReports
                .allows(&Url::parse("https://logistics.amazon.com/report").unwrap())
        );
        server.abort();
    }
}
