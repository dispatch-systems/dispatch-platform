//! Short-lived, audience-bound access to the separately hosted design workspace.
use crate::{
    Result,
    contracts::PlaygroundAccess,
    crypto,
    db::{Store, now},
    http::{
        input::{Input, Reply},
        route::{PlatformOwner, Route, User, read},
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

pub fn routes() -> Vec<Route> {
    vec![read(
        "/api/platform/design-playground",
        PlatformOwner,
        access,
    )]
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
