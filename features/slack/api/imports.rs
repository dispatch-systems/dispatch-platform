//! Import Files' endpoints, on each workspace's settings: what it's saving, linking channels to
//! folders, and browsing folders to choose one. Each is for those who manage Slack, and the
//! handlers check they see the folders where the DSP keeps its files; each waits outside the
//! database, as it asks where the files are kept, and checks the member again before it writes.
use super::routes::{SETTINGS, channel, team};
use crate::backend::imports::links;
use dispatch_core::{
    Result, State, ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional},
        route::{Dsp, Grant, Route, async_get, async_post},
    },
};
use std::sync::Arc;

pub fn routes() -> Vec<Route> {
    vec![
        async_get(
            "/api/dsp/slack/workspaces/{team}/imports",
            SETTINGS,
            overview,
        ),
        async_get(
            "/api/dsp/slack/workspaces/{team}/imports/folders",
            SETTINGS,
            folders,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/imports/links",
            SETTINGS,
            link,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/imports/links/{channel}/folder",
            SETTINGS,
            move_to,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/imports/links/{channel}/remove",
            SETTINGS,
            unlink,
        ),
    ]
}

/// A folder's ID where the DSP keeps its files, as Google Drive writes them.
fn folder(id: &str) -> Result<String> {
    ensure(
        (1..=128).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid_input",
        400,
    )?;
    Ok(id.to_owned())
}

async fn overview(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&links::overview(&state, &c, &team).await?)
}
async fn folders(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["folder"])?;
    let team = team(input.param("team"))?;
    let within = optional(&input.query, "folder", |q, key| {
        folder(v::text(q, key, 1, 128)?)
    })?;
    Reply::of(&links::browse(&state, &c, &team, within).await?)
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
    Reply::of(&links::link(&state, c, access, team, linked, into).await?)
}
async fn move_to(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["folder"])?;
    let (team, linked) = (team(input.param("team"))?, channel(input.param("channel"))?);
    let into = folder(v::text(&input.body, "folder", 1, 128)?)?;
    Reply::of(&links::move_to(&state, c, access, team, linked, into).await?)
}
async fn unlink(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let (team, linked) = (team(input.param("team"))?, channel(input.param("channel"))?);
    Reply::of(&links::unlink(&state, c, access, team, linked).await?)
}
