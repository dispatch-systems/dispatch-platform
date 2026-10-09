//! What the dashboard polls: readiness, its own updates, collection progress and presence.
use crate::{
    Error, Result, State,
    accounts::Context,
    foundation::{crypto, validate as v},
    manifest::registry,
    server::http::{
        browser_update_ready,
        input::{Input, Reply},
        route::{Dsp, Grant, Route, async_get, async_post, probe},
    },
    tenancy::roles,
};
use serde_json::json;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

// Collection progress refreshes the pages that follow it live, as their features declare.
static LIVE_UPDATES: LazyLock<String> = LazyLock::new(|| {
    let features = registry().features.iter();
    let permissions: Vec<_> = features.flat_map(|feature| feature.live).copied().collect();
    permissions.join("|")
});

pub fn routes() -> Vec<Route> {
    // With no feature that follows collections live, nothing may follow their progress.
    let live = (!LIVE_UPDATES.is_empty()).then(|| {
        async_get(
            "/api/dsp/collection-updates",
            Dsp(LIVE_UPDATES.as_str()),
            collection_updates,
        )
    });
    [
        probe("/api/health", health),
        probe("/api/browser-update", browser_update),
    ]
    .into_iter()
    .chain(live)
    .chain([async_post(
        "/api/dsp/presence",
        Dsp(roles::ACCESS),
        presence,
    )])
    .collect()
}

fn health(state: &State) -> Reply {
    Reply::json(json!({
        "status":"ready",
        "environment":state.config.environment,
        "release":state.config.release,
        "runtime":"rust"
    }))
}

fn browser_update(state: &State) -> Reply {
    let ready = browser_update_ready(&state.config);
    Reply::json(json!({"build":crypto::sha(state.config.release.as_bytes()),"ready":ready}))
}

// Long polling carries the same signed-view header as every other read.
// No DB slot or transaction stays open while a client waits.
async fn collection_updates(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    v::fields(&input.query, &["after"])?;
    let after = v::text(&input.query, "after", 0, 100)?.to_owned();
    let auth = input.clone();
    let dsp = state
        .read(move |db| Ok(access.authorize(db, &auth)?.dsp.id))
        .await?;
    let _slot = state
        .updates
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new("platform_busy", 503))?;
    let mut updates = state.updates.subscribe(&dsp);
    if state.updates.token(&updates) == after {
        let _ = tokio::time::timeout(Duration::from_secs(20), updates.changed()).await;
    }
    let revision = state.updates.token(&updates);
    // Permission changes and expired sessions take effect during an open wait.
    let followed = state
        .read(move |db| access.authorize(db, &input).map(|c| followed(&c)))
        .await?;
    let changes = state
        .updates
        .changes(&dsp, &after, &revision, |kind| followed.contains(&kind));
    Reply::of(&crate::collection::api::types::CollectionUpdates { revision, changes })
}

/// The kinds of job whose finished collections a member may hear of: those its features keep
/// whose `live` permissions the member holds.
fn followed(c: &Context) -> Vec<&'static str> {
    let features = registry().features.iter();
    let following = features.filter(|feature| feature.live.iter().any(|p| c.can(p)));
    following
        .flat_map(|feature| feature.keeps.iter().map(|keeper| keeper.keeps()))
        .collect()
}

// Heartbeats only touch memory, so they stay off the platform write lock.
async fn presence(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    v::fields(&input.body, &["tab", "state"])?;
    let tab = v::text(&input.body, "tab", 1, 64)?.to_owned();
    let active = match v::choice(&input.body, "state", &["active", "idle", "gone"])? {
        "gone" => None,
        state => Some(state == "active"),
    };
    let member = state
        .read(move |db| {
            let c = access.authorize(db, &input)?;
            // A platform owner looking into a DSP is never shown to its team.
            Ok((!c.auth.user.platform_owner).then(|| {
                (
                    c.dsp.id.as_str().to_owned(),
                    c.auth.user.id.as_str().to_owned(),
                )
            }))
        })
        .await?;
    if let Some((dsp, user)) = member {
        state.presence.beat(&dsp, &user, &tab, active);
    }
    Ok(Reply::ok())
}
