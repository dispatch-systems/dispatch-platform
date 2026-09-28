//! Short-lived, audience-bound access to the separately hosted design workspace.
use crate::{
    Error, Result, State,
    contracts::PlaygroundAccess,
    crypto,
    db::{Store, now},
    http::{
        input::{Input, Reply},
        route::{Grant, PlatformOwner, Route, User, async_get, async_post, read},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/platform/design-playground", PlatformOwner, access),
        async_get(
            "/api/platform/design-playground/status",
            PlatformOwner,
            status,
        ),
        async_post(
            "/api/platform/design-playground/start",
            PlatformOwner,
            start,
        ),
    ]
}

async fn status(state: Arc<State>, input: Input, access: PlatformOwner) -> Result<Reply> {
    let auth = input.clone();
    state.read(move |db| access.authorize(db, &auth)).await?;
    let status = crate::playground::status(state.config.playground.as_ref()).await;
    state.read(move |db| access.authorize(db, &input)).await?;
    Reply::of(&status)
}

async fn start(state: Arc<State>, input: Input, access: PlatformOwner) -> Result<Reply> {
    let auth = input.clone();
    state.read(move |db| access.authorize(db, &auth)).await?;
    crate::validate::fields(&input.body, &[])?;
    let config = state
        .config
        .playground
        .as_ref()
        .ok_or_else(|| Error::new("playground_start_unavailable", 409))?;
    let status = crate::playground::start(config).await?;
    state.read(move |db| access.authorize(db, &input)).await?;
    Reply::of(&status)
}

fn access(db: &Store, owner: &User, _: &Input) -> Result<Reply> {
    let Some(config) = &db.config.playground else {
        return Reply::of(&PlaygroundAccess {
            origin: None,
            ticket: None,
            expires_at: None,
        });
    };
    let expires = now() + 30_000;
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({
        "aud": config.origin,
        "iss": db.config.origin,
        "sub": crypto::sha(owner.actor()),
        "exp": expires,
        "jti": crypto::token()?,
    }))?);
    let signature = crypto::sign(config.key.as_bytes(), &payload);
    Reply::of(&PlaygroundAccess {
        origin: Some(config.origin.clone()),
        ticket: Some(format!("{payload}.{signature}")),
        expires_at: Some(expires),
    })
}
