//! Sign in with Dispatch: the discovery documents, the OAuth endpoints an MCP client talks
//! to, and the platform owner's answer to an app asking to connect. The OAuth endpoints sit
//! outside `/api/`, whose JSON-only and Origin rules would refuse every client's form posts;
//! each reads its own request and answers as RFC 6749 says. Their rows feed no cached read,
//! so they write as bookkeeping. The owner also opens the pairing window here, and chooses
//! the kinds of app that may connect.
use crate::{
    Result, State,
    agents::oauth::{self, Answer, Query, Refusal},
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
    http::{HeaderValue, Method, StatusCode, header, request::Parts},
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
        // Approving makes something that acts for the owner, so it asks for recent
        // verification as making a key does; refusing never does.
        write(
            "/api/platform/oauth/requests/{id}/approve",
            PlatformOwner,
            approve,
        ),
        write(
            "/api/platform/oauth/requests/{id}/deny",
            PlatformRoutine,
            deny,
        ),
        read("/api/platform/oauth/pairing", PlatformRoutine, pairing),
        // Opening the window lets apps only ask; approving still asks for recent verification.
        write("/api/platform/oauth/pairing", PlatformRoutine, open_pairing),
        read("/api/platform/oauth/apps", PlatformRoutine, apps),
        // Letting a kind of app connect changes who may ask, so it asks for recent verification.
        write("/api/platform/oauth/apps", PlatformOwner, allow_app),
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
            if let Some(value) = authorized.browser
                && let Ok(cookie) =
                    browser_cookie(state.config.development, &value, BROWSER_SECONDS).parse()
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
/// The cookie naming the request this browser brought and the nonce it was given for it.
/// `SameSite=Lax`, since the browser arrives from the app's own site, and otherwise like the
/// session's: `__Host-` and `Secure` except in development, which serves plain http.
fn browser_name(development: bool) -> &'static str {
    if development {
        "dispatch_oauth_request"
    } else {
        "__Host-dispatch_oauth_request"
    }
}
fn browser_cookie(development: bool, value: &str, seconds: i64) -> String {
    let secure = if development { "" } else { "; Secure" };
    format!(
        "{}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{secure}",
        browser_name(development)
    )
}
/// The nonce this browser holds for the request the path names. A browser holding none,
/// or one for another request, did not bring this request: `wrong_browser`.
fn this_browser<'a>(user: &User, input: &'a Input) -> Result<&'a str> {
    let prefix = format!("{}=", browser_name(user.state.config.development));
    let mut held = input
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter_map(|part| part.strip_prefix(prefix.as_str()));
    let only = match (held.next(), held.next()) {
        (Some(value), None) => value,
        _ => "",
    };
    match only.split_once('.') {
        Some((id, nonce)) if id == input.param("id") && !nonce.is_empty() => Ok(nonce),
        _ => Err(crate::Error::new("wrong_browser", 403)),
    }
}

async fn token(state: Arc<State>, request: Request) -> Response {
    let answer = match form(&state, request, "/oauth/token").await {
        Ok((_, form)) => state
            .run_bookkeeping(move |db| Ok(db.oauth_token(&form)))
            .await
            .map_err(Refusal::from)
            .and_then(|answer| answer),
        Err(refusal) => Err(refusal),
    };
    match answer {
        Ok(tokens) => answered(200, tokens),
        Err(refusal) => refused("token", refusal),
    }
}

async fn register(state: Arc<State>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let read = async {
        let ip = middleware::address(&state, &parts, "/oauth/register")?;
        let body = middleware::bytes(body).await?;
        Ok::<_, crate::Error>((ip, body))
    };
    let (ip, body) = match read.await {
        Ok(read) => read,
        Err(error) => return refused("register", error.into()),
    };
    let Some(metadata) = content_type(&parts, "application/json")
        .then(|| serde_json::from_slice::<Value>(&body).ok())
        .flatten()
    else {
        return refused(
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
        Err(refusal) => refused("register", refusal),
    }
}

/// Always 200 for any token, known or not (RFC 7009 §2.2).
async fn revoke(state: Arc<State>, request: Request) -> Response {
    let answer = match form(&state, request, "/oauth/revoke").await {
        Ok((_, form)) => state
            .run_bookkeeping(move |db| Ok(db.revoke_oauth(&form)))
            .await
            .map_err(Refusal::from)
            .and_then(|answer| answer),
        Err(refusal) => Err(refusal),
    };
    match answer {
        Ok(()) => answered(200, json!({})),
        Err(refusal) => refused("revoke", refusal),
    }
}

/// A form post's parameters with the client's address, or why it is no form post.
async fn form(state: &State, request: Request, pattern: &'static str) -> Answer<(String, Query)> {
    let (parts, body) = request.into_parts();
    let ip = middleware::address(state, &parts, pattern)?;
    let body = middleware::bytes(body).await?;
    if !content_type(&parts, "application/x-www-form-urlencoded") {
        return Err(Refusal::new(
            "invalid_request",
            "The body must be application/x-www-form-urlencoded",
        ));
    }
    Ok((ip, Query::parse(&body)))
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
fn refused(endpoint: &str, refusal: Refusal) -> Response {
    observability::event(
        "warn",
        "oauth.refused",
        json!({"endpoint":endpoint,"error":refusal.error,"description":refusal.description}),
    );
    let body = json!({"error":refusal.error,"error_description":refusal.description});
    middleware::noted(answered(refusal.status, body), refusal.error)
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
    Ok(Reply::of(&redirect)?.cookie(browser_cookie(owner.state.config.development, "", 0)))
}
fn deny(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    let browser = this_browser(owner, input)?;
    v::fields(&input.body, &[])?;
    let redirect = db.deny_oauth(input.param("id"), browser)?;
    Ok(Reply::of(&redirect)?.cookie(browser_cookie(owner.state.config.development, "", 0)))
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
mod tests {
    use super::*;

    #[test]
    fn the_request_cookie_is_the_sessions_kind_but_survives_the_trip_from_the_app() {
        assert_eq!(
            browser_cookie(false, "authreq_1.n", BROWSER_SECONDS),
            "__Host-dispatch_oauth_request=authreq_1.n; Path=/; HttpOnly; SameSite=Lax; \
             Max-Age=600; Secure"
        );
        assert_eq!(
            browser_cookie(true, "", 0),
            "dispatch_oauth_request=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
        );
    }
}
