//! The HTTP layer. `table` lists core's endpoints, each part's next to its handlers in its
//! `api/`, then every feature's from its manifest; `route` is how they are registered,
//! `middleware` is what every request passes through, and whatever matches no route is an
//! asset or `unmatched`.
mod assets;
pub mod input;
pub(crate) mod middleware;
pub mod proxy;
pub mod route;
mod unmatched;
pub mod upload;

pub use assets::{Asset, assets, browser_update_ready};
pub use route::{Access, Route, Work, needs_recent_verification};
pub use unmatched::unmatched_areas;

use crate::{State, foundation::observability, manifest::registry};
use axum::{
    Router,
    extract::{Request, State as AxumState},
    http::Method,
    response::Response,
    routing::{MethodFilter, MethodRouter, any},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

/// Every registered route: core's, then every registered feature's, in the registry's order.
/// `app/tests/backend/integration/http_routes.rs` holds the expected list.
pub fn table() -> Vec<Route> {
    use crate::{accounts, collection, mcp, platform_owner, server, tenancy};
    let core = [
        server::api::routes::routes(),
        accounts::api::routes::sign_in::routes(),
        accounts::api::routes::security::routes(),
        tenancy::api::routes::routes(),
        platform_owner::api::routes::platform::routes(),
        mcp::api::routes::agent_api::routes(),
        mcp::api::routes::oauth::routes(),
        collection::api::routes::jobs::routes(),
        collection::api::routes::connections::routes(),
        platform_owner::api::routes::audit::routes(),
    ];
    let features = registry().features.iter().map(|feature| (feature.routes)());
    core.into_iter().chain(features).flatten().collect()
}

/// The route the request log names a request by until its route reads it: a logged route's
/// pattern, or else what `observability::route` makes of the path.
pub fn request_label(path: &str) -> &'static str {
    static LOGGED: LazyLock<Vec<route::LogLabel>> =
        LazyLock::new(|| table().iter().filter_map(Route::log_label).collect());
    LOGGED
        .iter()
        .find_map(|logged| logged.names(path))
        .unwrap_or_else(|| observability::route(path))
}

pub fn router(state: Arc<State>) -> Router {
    let mut paths: BTreeMap<String, MethodRouter<Arc<State>>> = BTreeMap::new();
    for route in table() {
        let route = Arc::new(route);
        let filter = MethodFilter::try_from(route.method.clone()).expect("a routable method");
        // An empty segment where a parameter belongs reaches the same handler, whose
        // own validation then answers, as it did before paths were matched by a table.
        let empty = route
            .path
            .split('/')
            .map(|part| if part.starts_with('{') { "" } else { part })
            .collect::<Vec<_>>()
            .join("/");
        let mut served = vec![route.path.to_owned()];
        if empty != route.path {
            served.push(empty);
        }
        for path in served {
            let route = route.clone();
            let handler = move |AxumState(state): AxumState<Arc<State>>, request: Request| {
                let route = route.clone();
                async move { route.serve(state, request).await }
            };
            // Another method on a known path is as unknown as any other path: HEAD never
            // falls through to GET, and `any` keeps axum from naming the methods in `Allow`.
            let methods = paths.remove(&path).unwrap_or_else(|| {
                any(unmatched::unmatched).on(MethodFilter::HEAD, unmatched::unmatched)
            });
            paths.insert(path, methods.on(filter, handler));
        }
    }
    let router = paths
        .into_iter()
        .fold(Router::new(), |router, (path, methods)| {
            router.route(&path, methods)
        });
    router
        .fallback(fallback)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::pipeline,
        ))
        .with_state(state)
}

async fn fallback(AxumState(state): AxumState<Arc<State>>, request: Request) -> Response {
    let path = request.uri().path();
    let asset = [Method::GET, Method::HEAD].contains(request.method())
        && (path == "/" || path.starts_with("/assets/"));
    if asset {
        return assets::serve(&state, request.method(), path, request.headers())
            .unwrap_or_else(middleware::failure);
    }
    unmatched::unmatched(AxumState(state), request).await
}
