//! The answer to a path or method no route was registered for.
use super::{
    middleware,
    route::{Agent, Dsp, Grant, PlatformOwner},
    routes,
};
use crate::{Error, Result, State, db::Store};
use axum::{
    extract::{Request, State as AxumState},
    http::Method,
    response::Response,
};
use std::sync::Arc;

/// Always `not_found`, but the platform and DSP areas first ask for what their
/// routes ask for, so probing them tells a caller nothing they may not know. The app
/// keeps what each part of the DSP area asked for.
pub async fn unmatched(AxumState(state): AxumState<Arc<State>>, request: Request) -> Response {
    match refuse(state, request).await {
        Ok(never) => match never {},
        Err(error) => middleware::failure(error),
    }
}
async fn refuse(state: Arc<State>, request: Request) -> Result<std::convert::Infallible> {
    let input = middleware::input(&state, request, "").await?;
    let reading = input.method == Method::GET && !input.path.starts_with("/api/invitations/");
    let work = move |db: &Store| {
        if input.path.starts_with("/api/platform/") {
            PlatformOwner.authorize(db, &input)?;
        } else if input.path.starts_with("/api/dsp/") {
            let post = input.method == Method::POST;
            Dsp(routes::area_permission(&input.path, post)).authorize(db, &input)?;
        } else if input.path.starts_with("/api/v1/") {
            Agent::READ.authorize(db, &input)?;
        }
        Err(Error::new("not_found", 404))
    };
    if reading {
        state.read(work).await
    } else {
        state.run(work).await
    }
}
