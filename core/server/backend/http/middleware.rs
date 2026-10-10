//! What every request passes through before and after its route: the request
//! log, the security headers, the Host and Origin checks, and the parsing of a
//! request into an [`Input`].
use super::input::Input;
use crate::{
    Error, Result, State, ensure,
    foundation::{config::Site, crypto, observability},
    manifest::registry,
};
use axum::{
    body::{Body, Bytes, to_bytes},
    extract::{ConnectInfo, Request, State as AxumState},
    http::{HeaderName, Method, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};

const BODY_LIMIT: usize = 64 * 1024;
const BODY_TIMEOUT: Duration = Duration::from_secs(15);

// The error code of a failed request, carried to the request log.
#[derive(Clone)]
struct Failure(String);

pub fn failure(error: Error) -> Response {
    let code = error.code.clone();
    noted(error.into_response(), &code)
}
/// A response a handler made itself, with the error code the request log should carry.
pub fn noted(mut response: Response, code: &str) -> Response {
    response.extensions_mut().insert(Failure(code.to_owned()));
    response
}

pub async fn pipeline(
    AxumState(state): AxumState<Arc<State>>,
    mut request: Request,
    next: Next,
) -> Response {
    let started = std::time::Instant::now();
    let route = super::request_label(request.uri().path());
    let method = request.method().clone();
    let trace = observability::RequestTrace::default();
    request.extensions_mut().insert(trace.clone());
    let client = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .and_then(|peer| {
            state
                .config
                .trusted_proxy
                .client_ip(peer.0.ip(), request.headers())
                .ok()
        })
        .map(|ip| {
            crypto::sign(
                &state.key,
                &format!("client:{}:{ip}", crate::db::now() / 86400000),
            )
        });
    let request_id = match crypto::id("req") {
        Ok(id) => id,
        Err(error) => return error.into_response(),
    };
    let agent = request.uri().path().starts_with("/api/v1/");
    let mut response = match admit(&state, &request) {
        Ok(site) => {
            request.extensions_mut().insert(site);
            next.run(request).await
        }
        Err(error) => failure(error),
    };
    if agent {
        agent_challenge(&state, &mut response);
    }
    let status = response.status().as_u16();
    let level = match status {
        500.. => "error",
        400.. => "warn",
        _ => "info",
    };
    let error = response.extensions().get::<Failure>().map(|f| f.0.clone());
    let context = trace.lock().unwrap_or_else(|poison| poison.into_inner());
    observability::event(
        level,
        "http.request",
        json!({
            "requestId":request_id,
            "method":method.as_str(),
            "route":context.route.unwrap_or(route),
            "actorId":context.actor,
            "dspId":context.tenant,
            "account":context.account,
            "client":client,
            "bulk":context.bulk,
            "status":status,
            "elapsedMs":started.elapsed().as_millis(),
            "error":error
        }),
    );
    let headers = response.headers_mut();
    headers.insert("x-request-id", request_id.parse().unwrap());
    if state.config.origin.starts_with("https://") {
        headers.insert(
            "strict-transport-security",
            "max-age=31536000".parse().unwrap(),
        );
    }
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "same-origin"),
        ("x-frame-options", "DENY"),
        ("cross-origin-resource-policy", "same-origin"),
        (
            "permissions-policy",
            concat!(
                "accelerometer=(), camera=(), geolocation=(), gyroscope=(), microphone=(), ",
                "payment=(), usb=(), publickey-credentials-create=(self), ",
                "publickey-credentials-get=(self)"
            ),
        ),
    ] {
        headers.insert(HeaderName::from_static(name), value.parse().unwrap());
    }
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert("no-store".parse().unwrap());
    // A page that runs an outside service's scripts, as a window of its own, sets its own
    // rules for them; every other response gets the dashboard's.
    headers
        .entry("cross-origin-opener-policy")
        .or_insert("same-origin".parse().unwrap());
    let development = state.config.development;
    let csp = format!(
        "default-src 'self'; script-src 'self'{}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; connect-src 'self' blob:{}; font-src 'self'; object-src 'none'; \
         base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        if development { " 'unsafe-inline'" } else { "" },
        if development { " ws:" } else { "" }
    );
    headers
        .entry("content-security-policy")
        .or_insert(csp.parse().unwrap());
    response
}

// What an agent needs to recover: where and how to sign in, as the agents' piece words the
// challenge for the code it was refused with, and when to try again.
fn agent_challenge(state: &State, response: &mut Response) {
    let status = response.status().as_u16();
    let code = response
        .extensions()
        .get::<Failure>()
        .map(|failure| failure.0.clone());
    let headers = response.headers_mut();
    match status {
        401 => {
            if let Some(agents) = registry().agents
                && let Ok(value) = (agents.challenge)(&state.config, code.as_deref()).parse()
            {
                headers.insert(header::WWW_AUTHENTICATE, value);
            }
        }
        // Calls use a rolling minute. Sixty seconds is a safe upper bound even when the
        // oldest admitted call is about to leave the window.
        429 => {
            headers.insert(header::RETRY_AFTER, 60.into());
        }
        // The server is busy for a moment, as while a collection writes.
        503 => {
            headers.insert(header::RETRY_AFTER, 5.into());
        }
        _ => {}
    }
}

// Only one of the server's addresses, or this machine, may address the server, and only
// the pages of the address a request came to may send its API anything but a read.
fn admit(state: &State, request: &Request) -> Result<Site> {
    let header = |name| request.headers().get(name).and_then(|v| v.to_str().ok());
    let host = header(header::HOST).unwrap_or("");
    let site = state
        .config
        .site(host)
        .ok_or_else(|| Error::new("invalid_host", 400))?;
    let reads = [Method::GET, Method::HEAD, Method::OPTIONS];
    if request.uri().path().starts_with("/api/") && !reads.contains(request.method()) {
        // The agent API takes only a key, which is its own proof from wherever it runs, and
        // refuses any request carrying a session. The Origin check that guards a browser's
        // cookies has nothing to guard there, and a request without a key is told to bring
        // one, as MCP clients expect.
        let agent = request.uri().path().starts_with("/api/v1/");
        let origin = header(header::ORIGIN);
        ensure(
            agent || origin == Some(&state.config.site_origin(&site)),
            "invalid_origin",
            403,
        )?;
        // JSON, or a file for a route that takes uploads, which checks it is one.
        let kind = header(header::CONTENT_TYPE).and_then(|v| v.split(';').next());
        ensure(
            matches!(kind, Some("application/json" | "application/octet-stream")),
            "json_required",
            415,
        )?;
    }
    Ok(site)
}
/// Refuses a body that isn't JSON: every route's but an upload's.
pub(super) fn json_only(parts: &Parts) -> Result<()> {
    let kind = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next());
    let reads = [Method::GET, Method::HEAD, Method::OPTIONS];
    ensure(
        reads.contains(&parts.method) || kind != Some("application/octet-stream"),
        "json_required",
        415,
    )
}

/// Parses a request the way every route and the unmatched answer expect it:
/// only GET and POST exist, a query names each key once, and a body is JSON
/// of at most 64 KiB that arrives within 15 seconds.
pub async fn input(state: &State, request: Request, pattern: &'static str) -> Result<Input> {
    let (parts, body) = request.into_parts();
    let mut input = head(state, &parts, pattern)?;
    json_only(&parts)?;
    let body = bytes(body).await?;
    if input.method == Method::POST {
        input.body = serde_json::from_slice(&body)?;
    }
    if ["/api/auth/login", "/api/auth/forgot-password"].contains(&pattern) {
        input
            .trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .account = input
            .body
            .get("email")
            .and_then(Value::as_str)
            .filter(|email| email.len() <= 254)
            .map(|email| {
                crypto::sign(
                    &state.key,
                    &format!("account:{}", email.trim().to_lowercase()),
                )
            });
    }
    Ok(input)
}

/// What `input` reads before the body: the method, the query, the client's address and
/// the request's trace. A route that reads its own body, as the MCP server does, starts here.
pub fn head(state: &State, parts: &Parts, pattern: &'static str) -> Result<Input> {
    let known = [Method::GET, Method::POST].contains(&parts.method);
    ensure(known, "not_found", 404)?;
    let mut query = serde_json::Map::new();
    for (k, value) in url::form_urlencoded::parse(parts.uri.query().unwrap_or("").as_bytes()) {
        ensure(!query.contains_key(k.as_ref()), "invalid_input", 400)?;
        query.insert(k.into_owned(), json!(value));
    }
    let ip = address(state, parts, pattern)?;
    let site = parts
        .extensions
        .get::<Site>()
        .cloned()
        .ok_or_else(|| Error::new("invalid_host", 400))?;
    let trace = parts
        .extensions
        .get::<observability::RequestTrace>()
        .cloned()
        .unwrap_or_default();
    {
        let mut context = trace.lock().unwrap_or_else(|poison| poison.into_inner());
        context.route = Some(pattern);
        context.bulk =
            query.get("limit").is_some_and(|value| value == "all") || pattern.ends_with("/export");
    }
    Ok(Input {
        path: parts.uri.path().to_owned(),
        method: parts.method.clone(),
        headers: parts.headers.clone(),
        body: Value::Null,
        query: Value::Object(query),
        ip,
        site,
        trace,
        pattern,
    })
}

/// The client's address, for a handler that reads its own request; the request log then
/// names the route by its registered pattern.
pub fn address(state: &State, parts: &Parts, pattern: &'static str) -> Result<String> {
    if let Some(trace) = parts.extensions.get::<observability::RequestTrace>() {
        trace
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .route = Some(pattern);
    }
    let peer = parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|v| v.0.ip())
        .ok_or_else(|| Error::new("client_address_unavailable", 500))?;
    state.config.trusted_proxy.client_ip(peer, &parts.headers)
}

/// A request's body: at most 64 KiB, arriving within 15 seconds.
pub async fn bytes(body: Body) -> Result<Bytes> {
    tokio::time::timeout(BODY_TIMEOUT, to_bytes(body, BODY_LIMIT))
        .await
        .map_err(|_| Error::new("request_timeout", 408))?
        .map_err(|_| Error::new("request_too_large", 413))
}
