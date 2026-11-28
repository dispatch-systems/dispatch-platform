//! Requests for a demo: the address the public site sends each to, signed, and what a platform
//! owner reads and changes of them.
use crate::{
    Error, Result, State,
    db::{Store, now},
    ensure,
    foundation::{config::Site, validate as v},
    platform_owner::{
        api::types::DemoStatus,
        demo_requests::{Asked, secret, signed},
    },
    server::http::{
        input::{Input, Reply},
        middleware,
        route::{PlatformOwner, Route, User, protocol, read, write},
    },
};
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;

/// Where the site sends what a visitor asked, at the platform owner's address alone.
const INTAKE_PATH: &str = "/demo-requests";

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/platform/demo-requests", PlatformOwner, requests),
        read(
            "/api/platform/demo-requests/new",
            PlatformOwner,
            new_requests,
        ),
        write(
            "/api/platform/demo-requests/{id}/status",
            PlatformOwner,
            set_status,
        ),
        write(
            "/api/platform/demo-requests/{id}/delete",
            PlatformOwner,
            delete,
        ),
        protocol(Method::POST, INTAKE_PATH, |state, request| {
            Box::pin(intake(state, request))
        }),
    ]
}

fn requests(db: &Store, _: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.demo_requests()?)
}
fn new_requests(db: &Store, _: &User, _: &Input) -> Result<Reply> {
    Reply::of(&db.new_demo_requests()?)
}
fn set_status(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["status"])?;
    let status = input.body["status"]
        .as_str()
        .and_then(DemoStatus::parse)
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    db.set_demo_request_status(&owner.user.id, input.param("id"), status)?;
    // Every request as it now stands, which the page shows in place of what it had read.
    Reply::of(&db.demo_requests()?)
}
fn delete(db: &Store, owner: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    db.delete_demo_request(&owner.user.id, input.param("id"))?;
    // As a change of status answers: every request left.
    Reply::of(&db.demo_requests()?)
}

async fn intake(state: Arc<State>, request: Request) -> Response {
    match receive(&state, request).await {
        Ok(()) => Reply::json(json!({ "ok": true })).into_response(),
        Err(refused) => middleware::failure(refused),
    }
}
/// Keeps a request the site signed. Anywhere but the platform owner's address, or with no
/// secret to check one against, there is no such address.
async fn receive(state: &Arc<State>, request: Request) -> Result<()> {
    let (parts, body) = request.into_parts();
    middleware::address(state, &parts, INTAKE_PATH)?;
    let secret = secret(&state.config)
        .filter(|_| parts.extensions.get::<Site>() == Some(&Site::Admin))
        .ok_or_else(|| Error::new("not_found", 404))?;
    let body = middleware::bytes(body).await?;
    let body = std::str::from_utf8(&body).map_err(|_| Error::new("invalid_input", 400))?;
    ensure(
        signed(&secret, &parts.headers, body, now() / 1000),
        "demo_request_unsigned",
        401,
    )?;
    let sent = serde_json::from_str(body).map_err(|_| Error::new("invalid_input", 400))?;
    let asked = Asked::of(&sent)?;
    state.run(move |db| db.keep_demo_request(&asked)).await
}
