//! The Slack app itself, as Slack knows it: the permissions it asks each workspace for, the
//! events it's told of, where Slack reaches this server, and, from those, its manifest. Adding
//! to what Slack itself does there starts here: a permission in `SCOPES`, an event in `EVENTS`,
//! and then its handler, where `inbound` routes it. Another feature adds what it needs with an
//! integration, which `scopes` and `events` add to these.
use super::integrations;
use dispatch_core::{
    Result, ensure, foundation::config::Config, manifest::integrations::Integration,
};
use serde_json::{Value, json};

/// What the bot may do in a workspace for Slack itself, which whoever installs it there
/// approves: only what it uses. The hourly check reads the workspace's name with `team:read`. Its settings'
/// Channels list the public channels and the private ones the bot was invited to
/// (`channels:read`, `groups:read`), join public ones (`channels:join`) and leave either
/// (`channels:manage`, `groups:write`). Driver Match reads its members, with their email
/// addresses (`users:read`, `users:read.email`). The channels' bookmarks (`bookmarks:read`) say
/// which of a channel's folders holds each of its canvases, which Sync Canvases, being built,
/// will read: until it's ready, nothing reads them.
pub const SCOPES: &[&str] = &[
    "team:read",
    "channels:read",
    "groups:read",
    "channels:join",
    "channels:manage",
    "groups:write",
    "users:read",
    "users:read.email",
    "bookmarks:read",
];
/// What Slack tells the app of for Slack itself: its Home tab opened, someone joining the
/// workspace or changing their profile, and its being removed from a workspace or losing its
/// tokens there.
pub const EVENTS: &[&str] = &[
    "app_home_opened",
    "team_join",
    "user_change",
    "app_uninstalled",
    "tokens_revoked",
];

/// What the app asks each workspace's admin to let it do: what Slack itself uses, then what its
/// integrations need, each once. One added shows Update on each workspace's card until its
/// admin approves it.
pub fn scopes() -> Vec<&'static str> {
    with_integrations(SCOPES, |integration| integration.needs)
}
/// What the app asks Slack to tell it of: what Slack itself hears of, then what its
/// integrations do, each once.
pub fn events() -> Vec<&'static str> {
    with_integrations(EVENTS, |integration| integration.hears)
}
fn with_integrations(
    own: &[&'static str],
    theirs: fn(&Integration) -> &'static [&'static str],
) -> Vec<&'static str> {
    let mut all = own.to_vec();
    for (_, integration) in integrations::declared() {
        for each in theirs(integration) {
            if !all.contains(each) {
                all.push(each);
            }
        }
    }
    all
}

/// Where Slack sends the browser back after installing, on the admin's address.
pub const RETURN_PATH: &str = "/api/slack/return";
/// Where an install link starts installing, on the admin's address.
pub const INSTALL_PATH: &str = "/api/slack/install";
/// Where Slack sends its events, and what people do with the app's buttons, forms and
/// shortcuts: outside `/api/`, as no page of Dispatch's sends them.
pub const EVENTS_PATH: &str = "/slack/events";
pub const INTERACTIONS_PATH: &str = "/slack/interactions";

/// Runs `slack-manifest`, the operator's only Slack command: it prints this server's app as
/// Slack's manifest, to paste into Slack when making the app or changing it, so the app always
/// matches what the code asks for and listens at.
pub fn run(config: &Config, args: &[String]) -> Result<Value> {
    ensure(
        args.len() == 1 && args[0] == "slack-manifest",
        "usage_slack_manifest",
        400,
    )?;
    Ok(manifest(config))
}

/// This server's app: Dev's named apart from Production's, so the two are never mistaken for
/// each other in a workspace.
fn manifest(config: &Config) -> Value {
    let origin = &config.origin;
    let (name, handle) = if config.env().is_production() {
        ("Dispatch", "dispatch")
    } else {
        ("Dispatch Dev", "dispatch-dev")
    };
    json!({
        "display_information": {
            "name": name,
            "background_color": "#2055ed",
        },
        "features": {
            "app_home": {
                "home_tab_enabled": true,
                "messages_tab_enabled": false,
                "messages_tab_read_only_enabled": false,
            },
            "bot_user": {"display_name": handle, "always_online": true},
        },
        "oauth_config": {
            "redirect_urls": [format!("{origin}{RETURN_PATH}")],
            "scopes": {"bot": scopes()},
        },
        "settings": {
            "event_subscriptions": {
                "request_url": format!("{origin}{EVENTS_PATH}"),
                "bot_events": events(),
            },
            "interactivity": {
                "is_enabled": true,
                "request_url": format!("{origin}{INTERACTIONS_PATH}"),
            },
            "org_deploy_enabled": false,
            "socket_mode_enabled": false,
            "token_rotation_enabled": true,
        },
    })
}
