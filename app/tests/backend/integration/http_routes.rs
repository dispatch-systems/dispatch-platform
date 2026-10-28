//! Guardrails for the route table every owner's `api/routes` registers into.
// A build that leaves features out drops the tests that need them, and what only they use.
#![cfg_attr(not(feature = "default"), allow(dead_code, unused_imports))]
use dispatch_core::{
    server::http::{
        Access::{self, Agent, Dsp, Public},
        Route,
        Work::{self, Async, Read, Write},
    },
    tenancy::roles,
};
use std::{collections::BTreeSet, sync::LazyLock};

/// The route table, with the registry the permissions and routes are read from.
fn table() -> Vec<Route> {
    dispatch_backend::install();
    dispatch_core::server::http::table()
}

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
    // Slack's own requests, which no page of Dispatch's sends: each must carry Slack's
    // signature, made with the app's signing secret, which the feature checks before anything.
    "/slack/events",
    "/slack/interactions",
    // The image of someone's initials, which Slack fetches to show beside a message they sent:
    // two letters, and nothing a session or a DSP holds.
    "/communications/initials/1/{name}",
];

/// Every owner that registers routes, with the folder its `tests/api/routes.txt` is in: each
/// core part, the agents' piece, and each feature. The list is the proof that no route changed
/// its access unseen: a line each, as `describe` writes it, reviewed with the owner.
///
/// Adding an endpoint? Add its line to its owner's list. A reviewer then sees, in one line,
/// who can reach it, and a permission changed by accident fails a test.
fn owners() -> Vec<(String, Vec<Route>)> {
    use dispatch_core::{
        accounts, collection, manifest, platform_owner, server, settings, support, tenancy,
    };
    dispatch_backend::install();
    let registry = dispatch_core::manifest::registry();
    let mut owners = vec![
        ("core/accounts".to_owned(), accounts::api::routes()),
        ("core/collection".to_owned(), collection::api::routes()),
        ("core/manifest".to_owned(), manifest::api::routes()),
        (
            "core/platform_owner".to_owned(),
            platform_owner::api::routes(),
        ),
        ("core/server".to_owned(), server::api::routes()),
        ("core/settings".to_owned(), settings::api::routes()),
        ("core/support".to_owned(), support::api::routes()),
        ("core/tenancy".to_owned(), tenancy::api::routes()),
    ];
    owners.extend(
        registry
            .agents
            .map(|agents| ("mcp".to_owned(), (agents.routes)())),
    );
    owners.extend(
        registry
            .features
            .iter()
            .map(|feature| (format!("features/{}", feature.name), (feature.routes)())),
    );
    owners
}

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
/// Where an owner lists its own routes, a line each as `describe` writes it, so who can reach
/// them is reviewed with the owner.
fn listing(owner: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .expect("repository root")
        .join(owner)
        .join("tests/api/routes.txt")
}
/// What a listing writes as `<live>`, `<scheduling>`, `<collecting>` and `<reading>`: the
/// permissions the features' `live` fields name, those their keepers set schedules and collect
/// under, and those of their sections that show a live reader. The build's features decide
/// them, so core's listing names none of theirs. None without any.
static STANDS_FOR: LazyLock<[(&str, Option<String>); 4]> = LazyLock::new(|| {
    use dispatch_core::collection::schedules::Scheduled;
    dispatch_backend::install();
    let features = dispatch_core::manifest::registry().features.iter();
    let joined = |permissions: Vec<&str>| {
        (!permissions.is_empty()).then(|| format!("{:?}", permissions.join("|")))
    };
    let distinct = |mut permissions: Vec<&str>| {
        permissions.sort();
        permissions.dedup();
        joined(permissions)
    };
    let kept = |of: fn(Scheduled) -> &'static str| distinct(Scheduled::all().map(of).collect());
    let sections = dispatch_core::manifest::registry().sections();
    [
        (
            "<live>",
            joined(features.flat_map(|feature| feature.live).copied().collect()),
        ),
        (
            "<scheduling>",
            kept(|scheduled| scheduled.keeper.schedules().permission),
        ),
        (
            "<collecting>",
            kept(|scheduled| scheduled.keeper.permission()),
        ),
        (
            "<reading>",
            distinct(
                sections
                    .filter(|(_, section)| section.reader.is_some())
                    .map(|(_, section)| section.permission)
                    .collect(),
            ),
        ),
    ]
});
/// The routes an owner lists, without its comments, and with what each placeholder stands for:
/// a line naming one is left out when it stands for nothing, as its route is.
fn listed(owner: &str) -> BTreeSet<String> {
    std::fs::read_to_string(listing(owner))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            STANDS_FOR
                .iter()
                .try_fold(line.to_owned(), |line, (name, stands)| match stands {
                    _ if !line.contains(name) => Some(line),
                    Some(permissions) => Some(line.replace(name, permissions)),
                    None => None,
                })
        })
        .collect()
}

#[test]
fn each_owner_lists_exactly_the_routes_it_registers() {
    for (owner, routes) in owners() {
        let registered = described(routes.iter());
        let expected = listed(&owner);
        let missing: Vec<_> = expected.difference(&registered).collect();
        let unlisted: Vec<_> = registered.difference(&expected).collect();
        assert!(
            missing.is_empty() && unlisted.is_empty(),
            "{owner}: in {owner}/tests/api/routes.txt but not registered: {missing:#?}\n\
             registered but not listed there: {unlisted:#?}"
        );
    }
}

#[cfg(feature = "default")]
#[test]
fn the_route_table_is_exactly_what_the_owners_list() {
    let registered = described(&table());
    let expected: BTreeSet<String> = owners()
        .into_iter()
        .flat_map(|(owner, _)| listed(&owner))
        .collect();
    let missing: Vec<_> = expected.difference(&registered).collect();
    let unlisted: Vec<_> = registered.difference(&expected).collect();
    assert!(
        missing.is_empty() && unlisted.is_empty(),
        "listed but not registered: {missing:#?}\nregistered but no owner lists it: {unlisted:#?}"
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

// The versioned API admits agent keys and dedicated support service bearers, never
// browser sessions. Those credentials cannot call the dashboard's routes.
#[test]
fn only_agents_reach_the_versioned_api_and_nothing_else() {
    for route in table() {
        let versioned = route.path.starts_with("/api/v1/");
        let agent = matches!(route.access, Agent(_));
        assert_eq!(versioned, agent, "{} {}", route.method, route.path);
        if let Agent(access) = route.access {
            let admitted = match access {
                "read" | "operator" => !route.path.starts_with("/api/v1/support/"),
                "support worker" => {
                    route.path.starts_with("/api/v1/support/worker/")
                        && route.path != "/api/v1/support/worker/github/events"
                }
                "support github" => route.path == "/api/v1/support/worker/github/events",
                _ => false,
            };
            assert!(admitted, "{} requires {access}", route.path);
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
