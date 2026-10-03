//! Sign in with Dispatch: the discovery documents, the OAuth endpoints an MCP client talks
//! to, and the platform owner's answer to an app asking to connect. The OAuth endpoints sit
//! outside `/api/`, whose JSON-only and Origin rules would refuse every client's form posts;
//! each reads its own request and answers as RFC 6749 says. Their rows feed no cached read,
//! so they write as bookkeeping. The owner also opens the pairing window here, and chooses
//! the kinds of app that may connect.
use crate::{
    Result, State,
    agents::oauth::{
        self, Answer, Query, Refusal,
        limits::{Endpoint as Limited, Limits},
    },
    contracts::{self, OAuthAppChoice, OAuthAppId, OAuthApproval},
    db::Store,
    http::{
        input::{Input, Reply, optional_text},
        middleware,
        route::{
            PlatformOwner, PlatformRoutine, Route, Served, User, probe, protocol, read, write,
        },
    },
    observability, validate as v,
};
use axum::{
    Json,
    extract::Request,
    http::{HeaderName, HeaderValue, Method, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        probe("/.well-known/oauth-protected-resource", protected_resource),
        probe(
            "/.well-known/oauth-protected-resource/api/v1/mcp",
            protected_resource,
        ),
        probe(
            "/.well-known/oauth-authorization-server",
            authorization_server,
        ),
        // For clients that take the MCP endpoint for the issuer: the same document.
        probe(
            "/.well-known/oauth-authorization-server/api/v1/mcp",
            authorization_server,
        ),
        protocol(Method::GET, "/oauth/authorize", |state, request| {
            Box::pin(authorize(state, request)) as Served
        }),
        protocol(Method::POST, "/oauth/token", |state, request| {
            Box::pin(token(state, request)) as Served
        }),
        protocol(Method::POST, "/oauth/register", |state, request| {
            Box::pin(register(state, request)) as Served
        }),
        protocol(Method::POST, "/oauth/revoke", |state, request| {
            Box::pin(revoke(state, request)) as Served
        }),
        read("/api/platform/oauth/requests/{id}", PlatformOwner, request),
        // Approving makes an app that only ever reads what the owner allows, as making a key
        // does, so neither it nor refusing asks for recent verification.
        write(
            "/api/platform/oauth/requests/{id}/approve",
            PlatformRoutine,
            approve,
        ),
        write(
            "/api/platform/oauth/requests/{id}/deny",
            PlatformRoutine,
            deny,
        ),
        read("/api/platform/oauth/pairing", PlatformRoutine, pairing),
        // Opening the window lets apps only ask; the owner still approves each.
        write("/api/platform/oauth/pairing", PlatformRoutine, open_pairing),
        read("/api/platform/oauth/apps", PlatformRoutine, apps),
        // Letting a kind of app connect changes only who may ask.
        write("/api/platform/oauth/apps", PlatformRoutine, allow_app),
    ]
}

fn protected_resource(state: &State) -> Reply {
    Reply::json(oauth::protected_resource(&state.config))
}
fn authorization_server(state: &State) -> Reply {
    Reply::json(oauth::authorization_server(&state.config))
}

/// The browser arrives from the app. It leaves for the approval page, where the owner signs
/// in if they must, or for wherever a refusal belongs.
async fn authorize(state: Arc<State>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let ip = match middleware::address(&state, &parts, "/oauth/authorize") {
        Ok(ip) => ip,
        Err(error) => return middleware::failure(error),
    };
    if state.oauth_limits.admit(Limited::Authorize, &ip).is_err() {
        let response = Reply::redirect(format!(
            "{}/#authorize?error=rate_limited",
            oauth::issuer(&state.config)
        ))
        .into_response();
        return middleware::noted(response, "rate_limited");
    }
    let query = Query::parse(parts.uri.query().unwrap_or("").as_bytes());
    // Counted first, so no address makes Dispatch fetch an app's document more than it may;
    // then nothing is fetched for an app the owner does not let connect, or while the
    // pairing window is closed.
    let asked = query.clone();
    let admitted = state
        .run_bookkeeping(move |db| match db.throttle_authorize(&ip)? {
            Some(limited) => Ok(Err(limited)),
            None => db.admit_authorize(&asked),
        })
        .await;
    let app = match admitted {
        Ok(Ok(app)) => app,
        Ok(Err(location)) => return Reply::redirect(location).into_response(),
        Err(error) => return middleware::failure(error),
    };
    // An app's document is read before the database is opened, never while it is held.
    let document = match query.one("client_id") {
        Ok(Some(id)) if id.starts_with("https://") => Some(if app == OAuthAppId::Web {
            state.oauth.website(&state.config, id).await
        } else {
            state.oauth.client(&state.config, id).await
        }),
        _ => None,
    };
    let authorized = state
        .run_bookkeeping(move |db| db.authorize_oauth(&query, document))
        .await;
    match authorized {
        Ok(authorized) => {
            let mut response = Reply::redirect(authorized.location).into_response();
            // The browser that brought the request is the one that may answer it.
            if let Some((id, nonce)) = authorized.browser
                && let Ok(cookie) =
                    browser_cookie(state.config.development, &id, &nonce, BROWSER_SECONDS).parse()
            {
                response.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            response
        }
        Err(error) => middleware::failure(error),
    }
}

/// How long a browser holds its request: as long as the request waits.
const BROWSER_SECONDS: i64 = 10 * 60;
/// The cookie a browser holds for one request it brought, named for the request so that
/// several apps can be connecting from one browser at once, holding the nonce it was given.
/// `SameSite=Lax`, since the browser arrives from the app's own site, and otherwise like the
/// session's: `__Host-` and `Secure` except in development, which serves plain http.
fn browser_name(development: bool, id: &str) -> String {
    let host = if development { "" } else { "__Host-" };
    format!("{host}dispatch_oauth_request_{id}")
}
/// The cookie for request `id` holding `nonce`, kept `seconds`; an empty nonce and no
/// seconds clear it.
fn browser_cookie(development: bool, id: &str, nonce: &str, seconds: i64) -> String {
    let secure = if development { "" } else { "; Secure" };
    format!(
        "{}={nonce}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{secure}",
        browser_name(development, id)
    )
}
/// The nonce this browser holds for the request the path names. A browser holding none,
/// or more than one, did not bring this request: `wrong_browser`.
fn this_browser<'a>(user: &User, input: &'a Input) -> Result<&'a str> {
    let prefix = format!(
        "{}=",
        browser_name(user.state.config.development, input.param("id"))
    );
    let mut held = input
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter_map(|part| part.strip_prefix(prefix.as_str()));
    match (held.next(), held.next()) {
        (Some(nonce), None) if !nonce.is_empty() => Ok(nonce),
        _ => Err(crate::Error::new("wrong_browser", 403)),
    }
}

async fn token(state: Arc<State>, request: Request) -> Response {
    let answer = match form(&state, request, "/oauth/token", Limited::Token).await {
        Ok(form) => state
            .run_bookkeeping(move |db| Ok(db.oauth_token(&form)))
            .await
            .map_err(Refusal::from)
            .and_then(|answer| answer),
        Err(refusal) => Err(refusal),
    };
    match answer {
        Ok(tokens) => answered(200, tokens),
        Err(refusal) => refused(&state.oauth_limits, "token", refusal),
    }
}

async fn register(state: Arc<State>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let ip = match post_head(&state, &parts, "/oauth/register", Limited::Register) {
        Ok(ip) => ip,
        Err(error) => return refused(&state.oauth_limits, "register", error),
    };
    let body = match middleware::bytes(body).await {
        Ok(body) => body,
        Err(error) => return refused(&state.oauth_limits, "register", error.into()),
    };
    let Some(metadata) = content_type(&parts, "application/json")
        .then(|| serde_json::from_slice::<Value>(&body).ok())
        .flatten()
    else {
        return refused(
            &state.oauth_limits,
            "register",
            Refusal::new("invalid_client_metadata", "The body must be JSON"),
        );
    };
    let answer = state
        .run_bookkeeping(move |db| {
            if let Err(error) = db.throttle_ip("oauth-register", &ip, 20, 60 * 60 * 1000) {
                return Ok(Err(error.into()));
            }
            db.register_oauth_client(&metadata)
        })
        .await
        .map_err(Refusal::from)
        .and_then(|answer| answer);
    match answer {
        Ok(client) => answered(201, client),
        Err(refusal) => refused(&state.oauth_limits, "register", refusal),
    }
}

/// Always 200 for any token, known or not (RFC 7009 §2.2).
async fn revoke(state: Arc<State>, request: Request) -> Response {
    let answer = match form(&state, request, "/oauth/revoke", Limited::Revoke).await {
        Ok(form) => state
            .run_bookkeeping(move |db| Ok(db.revoke_oauth(&form)))
            .await
            .map_err(Refusal::from)
            .and_then(|answer| answer),
        Err(refusal) => Err(refusal),
    };
    match answer {
        Ok(()) => answered(200, json!({})),
        Err(refusal) => refused(&state.oauth_limits, "revoke", refusal),
    }
}

/// A form post's parameters, admitted before its body or the database is read. Browser form
/// submissions from another site are not OAuth clients and cannot read the answer; refusing
/// them prevents a webpage from enlisting visitors to spend Dispatch's OAuth budget.
async fn form(
    state: &State,
    request: Request,
    pattern: &'static str,
    endpoint: Limited,
) -> Answer<Query> {
    let (parts, body) = request.into_parts();
    post_head(state, &parts, pattern, endpoint)?;
    let body = middleware::bytes(body).await?;
    if !content_type(&parts, "application/x-www-form-urlencoded") {
        return Err(Refusal::new(
            "invalid_request",
            "The body must be application/x-www-form-urlencoded",
        ));
    }
    Ok(Query::parse(&body))
}
/// Admits a public OAuth POST before its body is polled. Browser pages from another site are
/// not protocol clients and may not enlist their visitors to spend the shared endpoint budget.
fn post_head(
    state: &State,
    parts: &Parts,
    pattern: &'static str,
    endpoint: Limited,
) -> Answer<String> {
    let ip = middleware::address(state, parts, pattern)?;
    if cross_site(parts, oauth::issuer(&state.config)) {
        return Err(Refusal::new(
            "invalid_request",
            "Cross-site browser form posts are not accepted",
        ));
    }
    state.oauth_limits.admit(endpoint, &ip)?;
    Ok(ip)
}
fn cross_site(parts: &Parts, origin: &str) -> bool {
    static SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
    parts
        .headers
        .get(&SEC_FETCH_SITE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
        || parts
            .headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value != origin)
}
fn content_type(parts: &Parts, expected: &str) -> bool {
    parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
}

/// An answer that may carry tokens, which nothing may keep (RFC 6749 §5.1).
fn answered(status: u16, body: Value) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
    let mut response = (status, Json(body)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}
/// An OAuth error (RFC 6749 §5.2), written down with the endpoint, never with a token.
fn refused(limits: &Limits, endpoint: &str, refusal: Refusal) -> Response {
    if let Some(sample) = limits.sample_refusal(endpoint, refusal.error) {
        observability::event(
            "warn",
            "oauth.refused",
            json!({"endpoint":endpoint,"error":refusal.error,
                "description":refusal.description,"sample":sample}),
        );
    }
    let body = json!({"error":refusal.error,"error_description":refusal.description});
    let mut response = middleware::noted(answered(refusal.status, body), refusal.error);
    if refusal.status == 429 {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("60"));
    }
    response
}

/// `?name=` asks what approving under that name would replace, as the owner edits it.
/// Only in the browser that brought the request, as approving and denying are.
fn request(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let browser = this_browser(owner, input)?;
    let name = optional_text(&input.query, "name", 80)?.trim();
    let name = (!name.is_empty()).then_some(name);
    Reply::of(&db.oauth_request(input.param("id"), name, browser)?)
}
/// Answered, the request is gone, and so is the browser's cookie for it.
fn approve(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let browser = this_browser(owner, input)?;
    let approval = OAuthApproval::parse(&input.body)?;
    let redirect = db.approve_oauth(owner.actor(), input.param("id"), &approval, browser)?;
    Ok(Reply::of(&redirect)?.cookie(forget_browser(owner, input)))
}
fn deny(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let browser = this_browser(owner, input)?;
    v::fields(&input.body, &[])?;
    let redirect = db.deny_oauth(input.param("id"), browser)?;
    Ok(Reply::of(&redirect)?.cookie(forget_browser(owner, input)))
}
/// Clears the cookie for the request just answered, and no other.
fn forget_browser(owner: &User, input: &Input) -> String {
    browser_cookie(owner.state.config.development, input.param("id"), "", 0)
}

fn pairing(db: &Store, _: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.oauth_pairing()?)
}
/// Opens the window for ten minutes, or keeps it open that long from now.
fn open_pairing(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&db.open_oauth_pairing(owner.actor())?)
}
fn apps(db: &Store, _: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.oauth_apps()?)
}
fn allow_app(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let choice: OAuthAppChoice = contracts::request(&input.body)?;
    Reply::of(&db.allow_oauth_app(owner.actor(), &choice)?)
}

#[cfg(test)]
#[path = "../../tests/backend/api/routes/oauth.rs"]
mod tests;
