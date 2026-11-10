//! One captured-owner admission per physical HTTP attempt. Reqwest's hidden retries and
//! redirects are disabled on these clients; OAuth connection setup keeps its own client.
use super::{called, unreachable};
use crate::backend::grant::Admission;
use dispatch_core::{Error, Result, foundation::observability};
use reqwest::{Method, Request, Response, StatusCode, Url, header};
use serde_json::json;
use std::{error::Error as _, time::Duration};
use tokio::time::Instant;

#[derive(Clone, Copy)]
pub(in crate::backend) enum Policy {
    Ordinary,
    Transfer,
    Picture,
    Upload,
}
impl Policy {
    fn timeout(self) -> Option<Duration> {
        match self {
            Self::Ordinary | Self::Picture => Some(Duration::from_secs(20)),
            Self::Transfer | Self::Upload => None,
        }
    }
    fn headers_timeout(self) -> Option<Duration> {
        match self {
            Self::Transfer | Self::Upload => Some(Duration::from_secs(60)),
            Self::Ordinary | Self::Picture => None,
        }
    }
    fn timeout_error(self) -> Error {
        if matches!(self, Self::Upload) {
            Error::new("upload_incomplete", 400)
        } else {
            timed_out()
        }
    }
    fn error(self, error: reqwest::Error) -> Error {
        if matches!(self, Self::Upload) && (error.is_body() || error.is_request()) {
            Error::new("upload_incomplete", 400)
        } else {
            unreachable(error)
        }
    }
}

pub(super) async fn response(
    admission: &Admission,
    builder: reqwest::RequestBuilder,
    policy: Policy,
) -> Result<Response> {
    let (client, request) = builder.build_split();
    let mut request = request.map_err(|e| policy.error(e))?;
    let mut started = None;
    let mut redirects = 0;
    let mut retries = 0;
    loop {
        // No authorization is retained across a response, retry, redirect or State wait.
        let limit = started.and_then(|start: Instant| {
            policy
                .timeout()
                .or(policy.headers_timeout())
                .map(|timeout| start + timeout)
        });
        if let Some(until) = limit {
            tokio::time::timeout_at(until, admission.check())
                .await
                .map_err(|_| policy.timeout_error())??;
        } else {
            admission.check().await?;
        }
        let start = *started.get_or_insert_with(Instant::now);
        // Keep one total deadline through the returned body. Transfer has one 60-second
        // header deadline across hops/retries, then the original per-read body timeout.
        *request.timeout_mut() = policy
            .timeout()
            .map(|timeout| (start + timeout).saturating_duration_since(Instant::now()));
        let replay = request.try_clone();
        let head = head(&request);
        let call = called(request.method(), request.url());
        let attempt_started = Instant::now();
        let answer = if let Some(timeout) = policy.headers_timeout() {
            tokio::time::timeout_at(start + timeout, client.execute(request))
                .await
                .map_err(|_| policy.timeout_error())?
        } else {
            client.execute(request).await
        };
        observability::event(
            "info",
            "documents.google",
            json!({"call": call, "elapsedMs": attempt_started.elapsed().as_millis() as u64,
                "status": answer.as_ref().ok().map(|response| response.status().as_u16())}),
        );
        let response = match answer {
            Err(error) if retries < 2 && protocol_nack(&error) && replay.is_some() => {
                retries += 1;
                request = replay.expect("the retry has a replayable body");
                continue;
            }
            answer => answer.map_err(|e| policy.error(e))?,
        };
        retries = 0;
        match redirected(&response, head, replay, policy, redirects)? {
            Some(next) => {
                redirects += 1;
                request = next;
            }
            None => return Ok(response),
        }
    }
}
fn timed_out() -> Error {
    Error::new("google_unreachable", 502)
}
fn head(request: &Request) -> Request {
    let mut head = Request::new(request.method().clone(), request.url().clone());
    *head.headers_mut() = request.headers().clone();
    *head.version_mut() = request.version();
    head
}

/// Match reqwest 0.13's default: at most two retries, replayable bodies only, and only a
/// remote graceful HTTP/2 GOAWAY or REFUSED_STREAM. Never retry provider HTTP statuses.
fn protocol_nack(error: &reqwest::Error) -> bool {
    let Some(legacy) = error.source() else {
        return false;
    };
    let Some(hyper) = legacy.source() else {
        return false;
    };
    let Some(error) = hyper
        .source()
        .and_then(|cause| cause.downcast_ref::<h2::Error>())
    else {
        return false;
    };
    error.is_remote()
        && ((error.is_go_away() && error.reason() == Some(h2::Reason::NO_ERROR))
            || (error.is_reset() && error.reason() == Some(h2::Reason::REFUSED_STREAM)))
}

fn redirected(
    response: &Response,
    head: Request,
    replay: Option<Request>,
    policy: Policy,
    followed: usize,
) -> Result<Option<Request>> {
    let empty = match response.status() {
        StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND => head.method() == Method::POST,
        StatusCode::SEE_OTHER => true,
        StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => false,
        _ => return Ok(None),
    };
    let previous = head.url().clone();
    let mut next = if empty {
        let mut request = head;
        if request.method() != Method::HEAD {
            *request.method_mut() = Method::GET;
        }
        for name in [
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
            header::CONTENT_ENCODING,
            header::TRANSFER_ENCODING,
        ] {
            request.headers_mut().remove(name);
        }
        request
    } else if let Some(replay) = replay {
        replay
    } else {
        // As reqwest does, return the redirect when a streamed body cannot be replayed.
        return Ok(None);
    };
    let Some(url) = response
        .headers()
        .get(header::LOCATION)
        .and_then(|location| std::str::from_utf8(location.as_bytes()).ok())
        .and_then(|location| previous.join(location).ok())
    else {
        return Ok(None);
    };
    if matches!(policy, Policy::Picture) {
        // A picture follows four redirects at most, each to one of Google's own hosts: any
        // other is handed back as the answer.
        if followed + 1 >= 5 || !picture_host(&url) {
            return Ok(None);
        }
    } else if followed >= 10 || !matches!(url.scheme(), "http" | "https") {
        return Err(timed_out());
    }
    if url.scheme() != previous.scheme()
        || url.host_str() != previous.host_str()
        || url.port_or_known_default() != previous.port_or_known_default()
    {
        for name in [
            header::AUTHORIZATION,
            header::COOKIE,
            header::PROXY_AUTHORIZATION,
            header::WWW_AUTHENTICATE,
        ] {
            next.headers_mut().remove(name);
        }
        next.headers_mut().remove("cookie2");
    }
    // A generated header from an earlier hop must not survive a later downgrade.
    next.headers_mut().remove(header::REFERER);
    // Reqwest's default Referer excludes credentials/fragments and HTTPS -> HTTP.
    if previous.scheme() != "https" || url.scheme() != "http" {
        let mut referer = previous;
        let _ = referer.set_username("");
        let _ = referer.set_password(None);
        referer.set_fragment(None);
        if let Ok(value) = header::HeaderValue::from_str(referer.as_str()) {
            next.headers_mut().insert(header::REFERER, value);
        }
    }
    *next.url_mut() = url;
    Ok(Some(next))
}

pub(in crate::backend) fn picture_host(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default();
    url.scheme() == "https"
        && (host.ends_with(".googleusercontent.com")
            || ["docs.google.com", "drive.google.com"].contains(&host))
}

#[cfg(test)]
#[path = "../tests/backend/transport.rs"]
mod tests;
