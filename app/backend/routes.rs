//! One file per area of core's API, each listing its routes next to their handlers. Each
//! feature's routes come from its manifest.
use super::route::Route;
use crate::{feature_manifests::team::api::TEAM, manifest::registry};

#[path = "../../core/mcp/api/routes/agent_api.rs"]
pub mod agent_api;
#[path = "../../core/platform_owner/api/routes/audit.rs"]
pub mod audit;
#[path = "../../core/collection/api/routes/connections.rs"]
pub mod connections;
#[path = "../../core/collection/api/routes/jobs.rs"]
pub mod jobs;
#[path = "../../core/server/api/routes.rs"]
pub mod live;
#[path = "../../core/mcp/api/routes/oauth.rs"]
pub mod oauth;
#[path = "../../core/platform_owner/api/routes/platform.rs"]
pub mod platform;
#[path = "../../core/collection/api/routes/schedules.rs"]
pub mod schedules;
#[path = "../../core/accounts/api/routes/security.rs"]
pub mod security;
#[path = "../../core/tenancy/api/routes.rs"]
pub mod session;
#[path = "../../core/accounts/api/routes/sign_in.rs"]
pub mod sign_in;

/// Core's routes, then every registered feature's, in the registry's order.
pub fn all() -> Vec<Route> {
    let core = [
        live::routes(),
        sign_in::routes(),
        security::routes(),
        session::routes(),
        platform::routes(),
        agent_api::routes(),
        oauth::routes(),
        jobs::routes(),
        connections::routes(),
        audit::routes(),
    ];
    let features = registry().features.iter().map(|feature| (feature.routes)());
    core.into_iter().chain(features).flatten().collect()
}

/// What each part of the DSP area asked for before routes carried their own access, for a
/// path no route matches. New routes never come here; this only keeps old answers the same.
pub fn area_permission(path: &str, post: bool) -> &'static str {
    let mut parts = path.split('/').skip(3);
    let (endpoint, detail) = (parts.next().unwrap_or(""), parts.next());
    match (endpoint, detail, post) {
        ("connections", ..) => "connections.manage",
        ("members", Some("invite"), true) | ("invitations", ..) => "members.invite",
        ("members", _, true) => "members.manage",
        ("roles", _, true) => "roles.manage",
        ("members" | "roles", _, false) => TEAM,
        ("profile", _, true) => "settings.manage",
        ("paycom", _, true) | ("schedules", ..) => "timecard.manage",
        ("jobs", Some("meal-breaks"), false) => "timecard.view",
        ("jobs", ..) | ("cortex", _, true) => "collections.run",
        ("dvic", Some("schedules"), _) => "dvic.manage",
        ("dvic", _, true) => "dvic.collect",
        ("dvic", ..) => "dvic.view",
        ("routes", Some("schedules"), _) => "routes.manage",
        ("routes", _, true) => "routes.collect",
        ("routes", ..) => "routes.view",
        ("scorecard", Some("schedules"), _) => "scorecard.manage",
        ("scorecard", _, true) => "scorecard.collect",
        ("scorecard", ..) => "scorecard.view",
        ("driver-match", ..) => "driver_match.manage",
        _ => "timecard.view",
    }
}
