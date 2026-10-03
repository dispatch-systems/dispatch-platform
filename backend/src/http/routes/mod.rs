//! One file per area of the API. Each lists its routes next to their handlers.
use super::route::Route;

#[path = "../../../../core/mcp/api/routes/agents.rs"]
pub mod agents;
#[path = "../../../../core/platform_owner/api/routes/audit.rs"]
pub mod audit;
#[path = "../../../../core/accounts/api/routes/auth.rs"]
pub mod auth;
#[path = "../../../../core/collection/api/routes/connections.rs"]
pub mod connections;
#[path = "../../../../features/driver_match/api/routes.rs"]
pub mod driver_match;
#[path = "../../../../features/dvic/api/routes.rs"]
pub mod dvic;
#[path = "../../../../core/collection/api/routes/jobs.rs"]
pub mod jobs;
#[path = "../../../../core/server/api/routes.rs"]
pub mod live;
#[path = "../../../../core/mcp/api/routes/oauth.rs"]
pub mod oauth;
#[path = "../../../../core/platform_owner/api/routes/platform.rs"]
pub mod platform;
#[path = "../../../../features/routes/api/routes.rs"]
pub mod routedata;
#[path = "../../../../core/collection/api/routes/schedules.rs"]
pub mod schedules;
#[path = "../../../../features/scorecard/api/routes.rs"]
pub mod scorecard;
#[path = "../../../../core/accounts/api/routes/security.rs"]
pub mod security;
#[path = "../../../../core/tenancy/api/routes.rs"]
pub mod session;
#[path = "../../../../features/settings/api/routes.rs"]
pub mod settings;
#[path = "../../../../features/team/api/routes.rs"]
pub mod team;
#[path = "../../../../features/timecard/api/routes.rs"]
pub mod timecard;
#[path = "../../../../features/uniforms/api/routes.rs"]
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
        schedules::routes(),
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
