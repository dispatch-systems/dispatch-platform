//! Sync Canvases' endpoints, on each workspace's settings: what it keeps, linking channels to
//! folders, choosing canvases of a channel's on their own, and browsing folders to choose one.
//! Each is for those who manage Slack, and the handlers check they see the folders where the
//! DSP keeps its files; each waits outside the database, as it asks Slack and where the files
//! are kept, and checks the member again before it writes.
use super::routes::{SETTINGS, channel, folder, team};
use crate::backend::{
    canvases::{LINKING, choices, overview::overview},
    channel_links,
};
use dispatch_core::{
    Error, Result, State, ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional},
        route::{Dsp, Grant, Route, async_get, async_post},
    },
};
use std::sync::Arc;

/// The most canvases one choice takes.
const CHOSEN: usize = 100;

pub fn routes() -> Vec<Route> {
    vec![
        async_get(
            "/api/dsp/slack/workspaces/{team}/canvases",
            SETTINGS,
            canvases,
        ),
        async_get(
            "/api/dsp/slack/workspaces/{team}/canvases/folders",
            SETTINGS,
            folders,
        ),
        async_get(
            "/api/dsp/slack/workspaces/{team}/canvases/channels/{channel}",
            SETTINGS,
            of_channel,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/canvases/links",
            SETTINGS,
            link,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/canvases/links/{channel}/folder",
            SETTINGS,
            move_to,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/canvases/links/{channel}/remove",
            SETTINGS,
            unlink,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/canvases/chosen",
            SETTINGS,
            choose,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/canvases/chosen/{canvas}/remove",
            SETTINGS,
            unchoose,
        ),
    ]
}

/// A canvas's ID, as Slack writes a file's.
fn canvas(id: &str) -> Result<String> {
    ensure(
        (2..=32).contains(&id.len())
            && id.starts_with('F')
            && id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
        "invalid_input",
        400,
    )?;
    Ok(id.to_owned())
}

async fn canvases(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&overview(&state, &c, &team).await?)
}
async fn folders(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["folder"])?;
    let team = team(input.param("team"))?;
    let within = optional(&input.query, "folder", |q, key| {
        folder(v::text(q, key, 1, 128)?)
    })?;
    Reply::of(&channel_links::browse(&state, &LINKING, &c, &team, within).await?)
}
async fn of_channel(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let (team, of) = (team(input.param("team"))?, channel(input.param("channel"))?);
    Reply::of(&choices::of_channel(&state, &c, &team, &of).await?)
}
async fn link(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["channel", "folder"])?;
    let team = team(input.param("team"))?;
    let linked = channel(v::text(&input.body, "channel", 2, 32)?)?;
    let into = optional(&input.body, "folder", |body, key| {
        folder(v::text(body, key, 1, 128)?)
    })?;
    channel_links::link(&state, &LINKING, &c, access, &team, &linked, into).await?;
    choices::linked(&state, &c.dsp.id, &team, &linked).await?;
    Reply::of(&overview(&state, &c, &team).await?)
}
async fn move_to(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["folder"])?;
    let (team, linked) = (team(input.param("team"))?, channel(input.param("channel"))?);
    let into = folder(v::text(&input.body, "folder", 1, 128)?)?;
    channel_links::move_to(&state, &LINKING, &c, access, &team, linked, into).await?;
    Reply::of(&overview(&state, &c, &team).await?)
}
async fn unlink(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let (team, linked) = (team(input.param("team"))?, channel(input.param("channel"))?);
    channel_links::unlink(&state, &LINKING, &c, access, &team, linked).await?;
    Reply::of(&overview(&state, &c, &team).await?)
}
async fn choose(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["channel", "canvases", "folder"])?;
    let team = team(input.param("team"))?;
    let of = channel(v::text(&input.body, "channel", 2, 32)?)?;
    let chosen = input.body["canvases"]
        .as_array()
        .filter(|chosen| (1..=CHOSEN).contains(&chosen.len()))
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(|id| canvas(id.as_str().unwrap_or("")))
        .collect::<Result<Vec<_>>>()?;
    let into = optional(&input.body, "folder", |body, key| {
        folder(v::text(body, key, 1, 128)?)
    })?;
    choices::choose(&state, &c, access, &team, &of, chosen, into).await?;
    Reply::of(&overview(&state, &c, &team).await?)
}
async fn unchoose(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let (team, chosen) = (team(input.param("team"))?, canvas(input.param("canvas"))?);
    choices::unchoose(&state, &c, access, &team, chosen).await?;
    Reply::of(&overview(&state, &c, &team).await?)
}
