//! One file per area of the API. Each lists its routes next to their handlers.
use super::route::Route;
use team::TEAM;

#[path = "../../core/mcp/api/routes/agents.rs"]
pub mod agents;
#[path = "../../core/platform_owner/api/routes/audit.rs"]
pub mod audit;
#[path = "../../core/accounts/api/routes/auth.rs"]
pub mod auth;
#[path = "../../core/collection/api/routes/connections.rs"]
pub mod connections;
#[path = "../../features/driver_match/api/routes.rs"]
pub mod driver_match;
#[path = "../../features/dvic/api/routes.rs"]
pub mod dvic;
#[path = "../../core/collection/api/routes/jobs.rs"]
pub mod jobs;
#[path = "../../core/server/api/routes.rs"]
pub mod live;
#[path = "../../core/mcp/api/routes/oauth.rs"]
pub mod oauth;
#[path = "../../core/platform_owner/api/routes/platform.rs"]
pub mod platform;
#[path = "../../features/routes/api/routes.rs"]
pub mod routedata;
#[path = "../../core/collection/api/routes/schedules.rs"]
pub mod schedules;
#[path = "../../features/scorecard/api/routes.rs"]
pub mod scorecard;
#[path = "../../core/accounts/api/routes/security.rs"]
pub mod security;
#[path = "../../core/tenancy/api/routes.rs"]
pub mod session;
#[path = "../../features/settings/api/routes.rs"]
pub mod settings;
#[path = "../../features/team/api/routes.rs"]
pub mod team;
#[path = "../../features/timecard/api/routes.rs"]
pub mod timecard;
#[path = "../../features/uniforms/api/routes.rs"]
pub mod uniforms;

pub fn all() -> Vec<Route> {
    [
        live::routes(),
        auth::routes(),
        security::routes(),
        session::routes(),
        platform::routes(),
        agents::routes(),
        oauth::routes(),
        team::routes(),
        timecard::routes(),
        scorecard::routes(),
        routedata::routes(),
        dvic::routes(),
        driver_match::routes(),
        jobs::routes(),
        connections::routes(),
        audit::routes(),
        settings::routes(),
        uniforms::routes(),
    ]
    .into_iter()
    .flatten()
    .collect()
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
