//! Guardrails for the route table every owner's `api/routes` registers into.
// A build that leaves features out drops the tests that need them, and what only they use.
#![cfg_attr(not(feature = "default"), allow(dead_code, unused_imports))]
use dispatch_core::{
    server::http::{
        Access::{self, Agent, Dsp, PlatformOwner, Public, Session},
        Route,
        Work::{self, Async, Memory, Read, Write},
    },
    tenancy::roles,
};
use std::collections::BTreeSet;

/// The route table, with the registry the permissions and routes are read from.
fn table() -> Vec<Route> {
    dispatch_backend::install();
    dispatch_core::server::http::table()
}

const TEAM: &str = "members.invite|members.manage|roles.manage";
const LIVE: &str = "timecard.view|collections.run";
const WAKES_SCHEDULER: bool = true;

type Row = (&'static str, &'static str, Access, Work, bool);

// The only paths outside `/api/`: OAuth's discovery documents and endpoints, which MCP clients
// find at fixed places and which must answer form posts from anywhere. All are public.
const OUTSIDE_THE_API: &[&str] = &[
    "/.well-known/oauth-protected-resource",
    "/.well-known/oauth-protected-resource/api/v1/mcp",
    "/.well-known/oauth-authorization-server",
    "/.well-known/oauth-authorization-server/api/v1/mcp",
    "/oauth/authorize",
    "/oauth/token",
    "/oauth/register",
    "/oauth/revoke",
];

// Every endpoint: who may call it, which database access its work runs under,
// and whether it wakes the scheduler. The list was written from the dispatcher
// this table replaced, so it is the proof that no route changed its permission.
//
// Adding an endpoint? Add its row here too. A reviewer then sees, in one line,
// who can reach it, and a permission changed by accident fails this test.
#[rustfmt::skip]
const INVENTORY: &[Row] = &[
    ("GET", "/api/health", Public, Memory, false),
    ("GET", "/api/browser-update", Public, Memory, false),
    ("POST", "/api/auth/login", Public, Async, false),
    ("POST", "/api/auth/logout", Session, Write, false),
    ("POST", "/api/auth/password", Session, Async, false),
    ("POST", "/api/auth/forgot-password", Public, Async, false),
    ("POST", "/api/auth/reset-password", Public, Async, false),
    ("GET", "/api/invitations/{token}", Public, Write, false),
    ("POST", "/api/invitations/{token}/accept", Public, Async, false),
    ("GET", "/api/auth/security/status", Session, Read, false),
    ("GET", "/api/auth/security/passkeys", Session, Read, false),
    ("POST", "/api/auth/security/passkeys/register/start", Session, Write, false),
    ("POST", "/api/auth/security/passkeys/register/finish", Session, Write, false),
    ("POST", "/api/auth/security/passkeys/verify/start", Session, Write, false),
    ("POST", "/api/auth/security/passkeys/verify/finish", Session, Write, false),
    ("POST", "/api/auth/security/passkeys/{id}/remove", Session, Write, false),
    ("POST", "/api/auth/security/authenticator/register/start", Session, Write, false),
    ("POST", "/api/auth/security/authenticator/register/finish", Session, Write, false),
    ("POST", "/api/auth/security/authenticator/verify", Session, Write, false),
    ("POST", "/api/auth/security/authenticator/remove", Session, Write, false),
    ("POST", "/api/auth/security/recover", Session, Write, false),
    ("POST", "/api/auth/security/recovery-codes", Session, Write, false),
    ("GET", "/api/auth/security/sessions", Session, Read, false),
    ("POST", "/api/auth/security/sessions/{id}/revoke", Session, Write, false),
    ("POST", "/api/auth/security/sessions/revoke-others", Session, Write, false),
    ("POST", "/api/auth/security/sessions/revoke-all", Session, Write, false),
    ("POST", "/api/auth/security/reauthenticate", Session, Async, false),
    ("GET", "/api/session", Session, Read, false),
    ("POST", "/api/session/dsp", Session, Write, false),

    ("GET", "/api/platform/agents", PlatformOwner, Read, false),
    ("POST", "/api/platform/agents/keys", PlatformOwner, Write, false),
    ("POST", "/api/platform/agents/keys/{id}", PlatformOwner, Write, false),
    ("POST", "/api/platform/agents/keys/{id}/revoke", PlatformOwner, Write, false),
    ("POST", "/api/platform/agents/revoke-all", PlatformOwner, Write, false),
    ("GET", "/api/platform/agents/activity", PlatformOwner, Read, false),
    ("GET", "/api/platform/agents/skill", PlatformOwner, Read, false),
    ("GET", "/api/platform/agents/openapi.json", PlatformOwner, Read, false),
    ("GET", "/.well-known/oauth-protected-resource", Public, Memory, false),
    ("GET", "/.well-known/oauth-protected-resource/api/v1/mcp", Public, Memory, false),
    ("GET", "/.well-known/oauth-authorization-server", Public, Memory, false),
    ("GET", "/.well-known/oauth-authorization-server/api/v1/mcp", Public, Memory, false),
    ("GET", "/oauth/authorize", Public, Async, false),
    ("POST", "/oauth/token", Public, Async, false),
    ("POST", "/oauth/register", Public, Async, false),
    ("POST", "/oauth/revoke", Public, Async, false),
    ("GET", "/api/platform/oauth/requests/{id}", PlatformOwner, Read, false),
    ("POST", "/api/platform/oauth/requests/{id}/approve", PlatformOwner, Write, false),
    ("POST", "/api/platform/oauth/requests/{id}/deny", PlatformOwner, Write, false),
    ("GET", "/api/platform/oauth/pairing", PlatformOwner, Read, false),
    ("POST", "/api/platform/oauth/pairing", PlatformOwner, Write, false),
    ("GET", "/api/platform/oauth/apps", PlatformOwner, Read, false),
    ("POST", "/api/platform/oauth/apps", PlatformOwner, Write, false),
    ("GET", "/api/v1/whoami", Agent("read"), Read, false),
    ("GET", "/api/v1/openapi.json", Agent("read"), Read, false),
    ("GET", "/api/v1/skill", Agent("read"), Read, false),
    ("POST", "/api/v1/mcp", Agent("read"), Async, false),
    ("GET", "/api/v1/mcp", Agent("read"), Async, false),
    ("GET", "/api/v1/status", Agent("read"), Read, false),
    ("GET", "/api/v1/metrics", Agent("read"), Read, false),
    ("GET", "/api/v1/drivers", Agent("read"), Read, false),
    ("GET", "/api/v1/drivers/{driver}", Agent("read"), Read, false),
    ("GET", "/api/v1/team", Agent("read"), Read, false),
    ("GET", "/api/v1/routes", Agent("read"), Read, false),
    ("GET", "/api/v1/routes/{route}", Agent("read"), Read, false),
    ("GET", "/api/v1/packages", Agent("read"), Read, false),
    ("GET", "/api/v1/feedback", Agent("read"), Read, false),
    ("GET", "/api/v1/safety", Agent("read"), Read, false),
    ("GET", "/api/v1/returns", Agent("read"), Read, false),
    ("GET", "/api/v1/scorecard", Agent("read"), Read, false),
    ("GET", "/api/v1/packages/{tracking}", Agent("read"), Read, false),
    ("GET", "/api/v1/timecards", Agent("read"), Read, false),
    ("GET", "/api/v1/meal-breaks", Agent("read"), Read, false),
    ("GET", "/api/v1/dvic", Agent("read"), Read, false),

    ("GET", "/api/platform/dsps", PlatformOwner, Read, false),
    ("POST", "/api/platform/dsps", PlatformOwner, Write, WAKES_SCHEDULER),
    ("POST", "/api/platform/dsps/{id}/retry", PlatformOwner, Write, WAKES_SCHEDULER),
    ("POST", "/api/platform/dsps/{id}/routes/reprocess", PlatformOwner, Write, false),
    ("POST", "/api/platform/dsps/{id}/status", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/support-visibility", PlatformOwner, Async, false),
    ("GET", "/api/platform/dsps/{id}/features", PlatformOwner, Read, false),
    ("POST", "/api/platform/dsps/{id}/features", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/remove", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/restore", PlatformOwner, Async, false),
    ("GET", "/api/platform/jobs", PlatformOwner, Read, false),
    ("GET", "/api/platform/audit", PlatformOwner, Read, false),
    ("POST", "/api/platform/audit/export", PlatformOwner, Write, false),
    ("GET", "/api/platform/health", PlatformOwner, Read, false),
    ("GET", "/api/platform/mail", PlatformOwner, Read, false),
    ("POST", "/api/platform/mail/{id}/retry", PlatformOwner, Write, false),
    ("POST", "/api/platform/mail/{id}/discard", PlatformOwner, Write, false),
    ("GET", "/api/platform/diagnostics", PlatformOwner, Read, false),
    ("POST", "/api/platform/diagnostics", PlatformOwner, Write, false),

    ("GET", "/api/dsp/uniforms", Dsp("uniforms.view"), Read, false),
    ("GET", "/api/dsp/uniforms/updates", Dsp("uniforms.view"), Async, false),
    ("GET", "/api/dsp/uniforms/history", Dsp("uniforms.view"), Read, false),
    ("POST", "/api/dsp/uniforms/initialize", Dsp("uniforms.manage"), Write, false),
    ("POST", "/api/dsp/uniforms", Dsp("uniforms.manage"), Write, false),
    ("POST", "/api/dsp/uniforms/{id}", Dsp("uniforms.manage"), Write, false),
    ("POST", "/api/dsp/uniforms/{id}/archive", Dsp("uniforms.manage"), Write, false),
    ("POST", "/api/dsp/uniforms/stock/{id}", Dsp("uniforms.adjust"), Write, false),
    ("GET", "/api/dsp/collection-updates", Dsp(LIVE), Async, false),
    ("POST", "/api/dsp/presence", Dsp("access"), Async, false),
    ("GET", "/api/dsp/employees", Dsp("timecard.view"), Read, false),
    ("GET", "/api/dsp/employees/{code}", Dsp("timecard.view"), Read, false),
    ("POST", "/api/dsp/employees/{code}/sync", Dsp("collections.run"), Write, false),
    ("GET", "/api/dsp/timecards", Dsp("timecard.view"), Read, false),
    ("GET", "/api/dsp/paycom/status", Dsp("timecard.view"), Read, false),
    ("GET", "/api/dsp/paycom/settings", Dsp("timecard.view"), Read, false),
    ("POST", "/api/dsp/paycom/settings", Dsp("timecard.manage"), Write, false),
    ("GET", "/api/dsp/paycom/meal-breaks", Dsp("timecard.view"), Read, false),
    ("GET", "/api/dsp/cortex/meal-breaks", Dsp("timecard.view"), Read, false),
    ("POST", "/api/dsp/cortex/meal-breaks/collect", Dsp("collections.run"), Write, false),
    ("GET", "/api/dsp/jobs", Dsp("collections.run"), Read, false),
    ("POST", "/api/dsp/jobs", Dsp("collections.run"), Write, false),
    ("POST", "/api/dsp/jobs/{id}/cancel", Dsp("collections.run"), Async, false),
    ("GET", "/api/dsp/jobs/meal-breaks", Dsp("timecard.view"), Read, false),
    ("POST", "/api/dsp/jobs/meal-breaks", Dsp("collections.run"), Write, false),
    ("GET", "/api/dsp/scorecard/weeks", Dsp("scorecard.view"), Read, false),
    ("GET", "/api/dsp/scorecard/jobs", Dsp("scorecard.view"), Read, false),
    ("POST", "/api/dsp/scorecard/collect", Dsp("scorecard.collect"), Write, false),
    ("POST", "/api/dsp/scorecard/jobs/{id}/cancel", Dsp("scorecard.collect"), Async, false),
    ("GET", "/api/dsp/scorecard/schedules", Dsp("scorecard.manage"), Read, false),
    ("POST", "/api/dsp/scorecard/schedules", Dsp("scorecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/scorecard/schedules/preview", Dsp("scorecard.manage"), Write, false),
    ("POST", "/api/dsp/scorecard/schedules/{key}", Dsp("scorecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/scorecard/schedules/{key}/enabled", Dsp("scorecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/scorecard/schedules/{key}/remove", Dsp("scorecard.manage"), Write, WAKES_SCHEDULER),
    ("GET", "/api/dsp/dvic/status", Dsp("dvic.view"), Read, false),
    ("GET", "/api/dsp/dvic/inspections", Dsp("dvic.view"), Read, false),
    ("POST", "/api/dsp/dvic/collect", Dsp("dvic.collect"), Write, false),
    ("POST", "/api/dsp/dvic/jobs/{id}/cancel", Dsp("dvic.collect"), Async, false),
    ("GET", "/api/dsp/dvic/schedules", Dsp("dvic.manage"), Read, false),
    ("POST", "/api/dsp/dvic/schedules", Dsp("dvic.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/dvic/schedules/preview", Dsp("dvic.manage"), Write, false),
    ("POST", "/api/dsp/dvic/schedules/{key}", Dsp("dvic.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/dvic/schedules/{key}/enabled", Dsp("dvic.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/dvic/schedules/{key}/remove", Dsp("dvic.manage"), Write, WAKES_SCHEDULER),
    ("GET", "/api/dsp/driver-match", Dsp("driver_match.manage"), Read, false),
    ("GET", "/api/dsp/driver-match/counts", Dsp("driver_match.manage"), Read, false),
    ("GET", "/api/dsp/driver-match/drivers/{code}", Dsp("driver_match.manage"), Read, false),
    ("POST", "/api/dsp/driver-match/merge", Dsp("driver_match.manage"), Write, false),
    ("POST", "/api/dsp/driver-match/split", Dsp("driver_match.manage"), Write, false),
    ("POST", "/api/dsp/driver-match/apart", Dsp("driver_match.manage"), Write, false),
    ("GET", "/api/dsp/routes/days", Dsp("routes.view"), Read, false),
    ("GET", "/api/dsp/routes/days/{day}", Dsp("routes.view"), Read, false),
    ("GET", "/api/dsp/routes/days/{day}/itineraries/{id}", Dsp("routes.view"), Read, false),
    ("GET", "/api/dsp/routes/packages/{tracking}", Dsp("routes.view"), Read, false),
    ("POST", "/api/dsp/routes/collect", Dsp("routes.collect"), Write, false),
    ("GET", "/api/dsp/routes/jobs", Dsp("routes.view"), Read, false),
    ("POST", "/api/dsp/routes/jobs/{id}/cancel", Dsp("routes.collect"), Async, false),
    ("GET", "/api/dsp/routes/retention", Dsp("routes.manage"), Read, false),
    ("POST", "/api/dsp/routes/retention", Dsp("routes.manage"), Write, false),
    ("GET", "/api/dsp/routes/schedules", Dsp("routes.manage"), Read, false),
    ("POST", "/api/dsp/routes/schedules", Dsp("routes.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/routes/schedules/preview", Dsp("routes.manage"), Write, false),
    ("POST", "/api/dsp/routes/schedules/{key}", Dsp("routes.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/routes/schedules/{key}/enabled", Dsp("routes.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/routes/schedules/{key}/remove", Dsp("routes.manage"), Write, WAKES_SCHEDULER),
    ("GET", "/api/dsp/schedules", Dsp("timecard.manage"), Read, false),
    ("POST", "/api/dsp/schedules", Dsp("timecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/schedules/preview", Dsp("timecard.manage"), Write, false),
    ("POST", "/api/dsp/schedules/{key}", Dsp("timecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/schedules/{key}/enabled", Dsp("timecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/schedules/{key}/remove", Dsp("timecard.manage"), Write, WAKES_SCHEDULER),
    ("POST", "/api/dsp/profile", Dsp("settings.manage"), Write, WAKES_SCHEDULER),
    ("GET", "/api/dsp/members", Dsp(TEAM), Read, false),
    ("POST", "/api/dsp/members/invite", Dsp("members.invite"), Write, false),
    ("POST", "/api/dsp/members/{id}", Dsp("members.manage"), Write, false),
    ("GET", "/api/dsp/invitations", Dsp("members.invite"), Read, false),
    ("POST", "/api/dsp/invitations/revoke", Dsp("members.invite"), Write, false),
    ("GET", "/api/dsp/roles", Dsp(TEAM), Read, false),
    ("POST", "/api/dsp/roles", Dsp("roles.manage"), Write, false),
    ("POST", "/api/dsp/roles/{id}", Dsp("roles.manage"), Write, false),
    ("POST", "/api/dsp/roles/{id}/remove", Dsp("roles.manage"), Write, false),
    ("GET", "/api/dsp/connections", Dsp("connections.manage"), Read, false),
    ("GET", "/api/dsp/connections/{provider}", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/save", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/check", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/disable", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/verify", Dsp("connections.manage"), Async, false),
    ("GET", "/api/dsp/connections/{provider}/screenshot", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/assist", Dsp("connections.manage"), Async, false),
    ("POST", "/api/dsp/connections/{provider}/submit", Dsp("connections.manage"), Async, false),
];

fn describe(rows: impl IntoIterator<Item = Row>) -> BTreeSet<String> {
    rows.into_iter()
        .map(|(method, path, access, work, wakes)| {
            let wakes = if wakes { " wakes-scheduler" } else { "" };
            format!("{method} {path} {access:?} {work:?}{wakes}")
        })
        .collect()
}

#[cfg(feature = "default")]
#[test]
fn the_route_table_is_exactly_the_inventory() {
    let registered = describe(table().iter().map(|route| {
        let method = if route.method == "GET" { "GET" } else { "POST" };
        let wakes = route.invalidates_schedules;
        (method, route.path, route.access, route.work, wakes)
    }));
    let expected = describe(INVENTORY.iter().copied());
    let missing: Vec<_> = expected.difference(&registered).collect();
    let unlisted: Vec<_> = registered.difference(&expected).collect();
    assert!(
        missing.is_empty() && unlisted.is_empty(),
        "in the inventory but not registered: {missing:#?}\nregistered but not in the inventory: {unlisted:#?}"
    );
}

#[test]
fn no_route_is_registered_twice_and_only_get_and_post_exist() {
    let mut seen = BTreeSet::new();
    for route in table() {
        assert!(
            ["GET", "POST"].contains(&route.method.as_str()),
            "{}",
            route.path
        );
        assert!(
            seen.insert((route.method.to_string(), route.path)),
            "{} {} is registered twice",
            route.method,
            route.path
        );
        assert!(
            route.path.starts_with("/api/")
                || (OUTSIDE_THE_API.contains(&route.path) && route.access == Public),
            "{}",
            route.path
        );
        // The Origin and CSRF checks rest on reads never changing anything for a session.
        if route.method == "GET" && route.work == Write {
            assert_eq!(route.access, Public, "{}", route.path);
        }
        assert!(
            !route.invalidates_schedules || route.work == Write,
            "{} wakes the scheduler without being a write",
            route.path
        );
    }
}

// The versioned API is for agents alone, signed in with a key, and agents reach nothing
// else: no session can call it, and no key can call the dashboard's routes.
#[test]
fn only_agents_reach_the_versioned_api_and_nothing_else() {
    for route in table() {
        let versioned = route.path.starts_with("/api/v1/");
        let agent = matches!(route.access, Agent(_));
        assert_eq!(versioned, agent, "{} {}", route.method, route.path);
        if let Agent(access) = route.access {
            assert!(["read", "operator"].contains(&access), "{}", route.path);
            // Calls are counted in memory, so a read is never a write. Only the MCP server
            // reads its own requests, and its tools only read.
            assert!(
                route.method != "GET"
                    || route.work == Read
                    || (route.work == Async && route.path == "/api/v1/mcp"),
                "{}",
                route.path
            );
        }
    }
}

#[test]
fn dsp_routes_only_name_permissions_that_exist() {
    for route in table() {
        if let Dsp(expression) = route.access {
            for permission in expression.split('|') {
                assert!(
                    roles::PERMISSIONS.contains(&permission) || permission == roles::ACCESS,
                    "{} {} requires unknown permission {permission:?}",
                    route.method,
                    route.path
                );
            }
        }
    }
}
