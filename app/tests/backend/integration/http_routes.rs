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

const LIVE: &str = "timecard.view|collections.run|dvic.view";
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

// Every endpoint of core and the collectors: who may call it, which database access its
// work runs under, and whether it wakes the scheduler. The list was written from the
// dispatcher this table replaced, so it is the proof that no route changed its permission.
// Each feature lists its own the same way, in its `tests/api/routes.txt`.
//
// Adding an endpoint? Add its row here, or its line to its feature's list. A reviewer then
// sees, in one line, who can reach it, and a permission changed by accident fails a test.
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
    ("GET", "/api/invitations/{token}/short-code", Public, Write, false),
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
    ("GET", "/api/site", Public, Read, false),
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

    ("GET", "/api/platform/dsps", PlatformOwner, Read, false),
    ("POST", "/api/platform/dsps", PlatformOwner, Write, WAKES_SCHEDULER),
    ("POST", "/api/platform/dsps/{id}/retry", PlatformOwner, Write, WAKES_SCHEDULER),
    ("POST", "/api/platform/dsps/{id}/status", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/support-visibility", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/code", PlatformOwner, Async, false),
    ("GET", "/api/platform/dsps/{id}/features", PlatformOwner, Read, false),
    ("POST", "/api/platform/dsps/{id}/features", PlatformOwner, Async, false),
    ("POST", "/api/platform/dsps/{id}/features/shown", PlatformOwner, Write, false),
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

    ("GET", "/api/dsp/collection-updates", Dsp(LIVE), Async, false),
    ("POST", "/api/dsp/presence", Dsp("access"), Async, false),
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

/// Routes as `describe` writes them.
fn described<'a>(routes: impl IntoIterator<Item = &'a Route>) -> BTreeSet<String> {
    describe(routes.into_iter().map(|route| {
        let method = if route.method == "GET" { "GET" } else { "POST" };
        let wakes = route.invalidates_schedules;
        (method, route.path, route.access, route.work, wakes)
    }))
}
/// Where each feature lists its own routes, a line each as `describe` writes it, so who can
/// reach them is reviewed with the feature.
fn listing(feature: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
        .join("features")
        .join(feature)
        .join("tests/api/routes.txt")
}
/// The routes a feature lists, without its comments.
fn listed(feature: &str) -> BTreeSet<String> {
    std::fs::read_to_string(listing(feature))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn each_feature_lists_exactly_the_routes_it_registers() {
    let table = table();
    for feature in dispatch_core::manifest::registry().features {
        // Its own routes, and its agent endpoints, which core serves from what it declares.
        let agents: BTreeSet<_> = feature.mcp.endpoints.iter().map(|e| e.path).collect();
        let registered = described(
            (feature.routes)()
                .iter()
                .chain(table.iter().filter(|route| agents.contains(route.path))),
        );
        let expected = listed(feature.name);
        let missing: Vec<_> = expected.difference(&registered).collect();
        let unlisted: Vec<_> = registered.difference(&expected).collect();
        assert!(
            missing.is_empty() && unlisted.is_empty(),
            "{}: in features/{}/tests/api/routes.txt but not registered: {missing:#?}\n\
             registered but not listed there: {unlisted:#?}",
            feature.name,
            feature.name
        );
    }
}

#[cfg(feature = "default")]
#[test]
fn the_route_table_is_exactly_the_inventory() {
    let registered = described(&table());
    // Core's routes, and the collectors', here; each feature's in its own folder.
    let mut expected = describe(INVENTORY.iter().copied());
    for feature in dispatch_core::manifest::registry().features {
        expected.extend(listed(feature.name));
    }
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
