//! The answer to a path or method no route was registered for.
use super::{
    middleware,
    route::{Dsp, Grant, PlatformOwner},
};
use crate::{Error, Result, State, db::Store, manifest::registry};
use axum::{
    extract::{Request, State as AxumState},
    http::Method,
    response::Response,
};
use std::sync::{Arc, OnceLock};

/// What each part of the DSP area asks of a path no route matches, by its path and whether
/// it is a POST. The app keeps it, and hands it over at startup.
static AREAS: OnceLock<fn(&str, bool) -> &'static str> = OnceLock::new();

/// Hands core what each part of the DSP area asks of a path no route matches. The first
/// call sets it; calling it again changes nothing.
pub fn unmatched_areas(areas: fn(&str, bool) -> &'static str) {
    AREAS.get_or_init(|| areas);
}

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
            let areas = AREAS
                .get()
                .expect("the app hands over the DSP area's permissions at startup");
            Dsp(areas(&input.path, post)).authorize(db, &input)?;
        } else if input.path.starts_with("/api/v1/")
            && let Some(agents) = registry().agents
        {
            (agents.authorize)(db, &input)?;
        }
        Err(Error::new("not_found", 404))
    };
    if reading {
        state.read(work).await
    } else {
        state.run(work).await
    }
}
