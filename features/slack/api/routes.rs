//! Slack's endpoints, each registered with its path and access. The server checks the session
//! and the permission before a handler runs; the handlers that call Slack wait outside the
//! database and check again before they write. Slack's own requests come to public addresses
//! outside `/api/`, each checked against Slack's signature before anything else.
use crate::backend::{
    app::{EVENTS_PATH, INSTALL_PATH, INTERACTIONS_PATH, RETURN_PATH},
    channels,
    connection::{self, Started},
    inbound, integrations, link, logo, pages,
    storage::{self, SlackStore},
};
use crate::integrations as own;
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional, optional_text},
        route::{Dsp, Grant, Member, Route, async_get, async_post, protocol, read, write},
    },
};
use serde_json::Value;
use std::sync::Arc;

/// Adding the DSP's workspaces and taking them out, as its other accounts on Settings' DSP
/// Connections are.
const CONNECTIONS: Dsp = Dsp("connections.manage");
/// What Dispatch does in each of them, on its settings.
pub(crate) const SETTINGS: Dsp = Dsp("slack.manage");
/// The workspaces themselves, for either.
const WORKSPACES: Dsp = Dsp("connections.manage|slack.manage");

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/slack/account", WORKSPACES, account),
        read("/api/dsp/slack/workspaces/{team}/logo", WORKSPACES, logo),
        async_get(
            "/api/dsp/slack/workspaces/{team}/channels",
            SETTINGS,
            list_channels,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/channels/{channel}/join",
            SETTINGS,
            join_channel,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/channels/{channel}/leave",
            SETTINGS,
            leave_channel,
        ),
        write(
            "/api/dsp/slack/workspaces/{team}/integrations/{integration}",
            SETTINGS,
            switch_integration,
        ),
        write("/api/dsp/slack/connect", CONNECTIONS, connect),
        async_post("/api/dsp/slack/connect/finish", CONNECTIONS, finish),
        write(
            "/api/dsp/slack/workspaces/{team}/rename",
            CONNECTIONS,
            rename,
        ),
        async_post(
            "/api/dsp/slack/workspaces/{team}/disconnect",
            CONNECTIONS,
            disconnect,
        ),
        write("/api/dsp/slack/link", CONNECTIONS, create_link),
        write("/api/dsp/slack/link/cancel", CONNECTIONS, cancel_link),
        write(
            "/api/dsp/slack/workspaces/{team}/link",
            CONNECTIONS,
            create_workspace_link,
        ),
        protocol(Method::GET, RETURN_PATH, |state, request| {
            Box::pin(returned(state, request))
        }),
        protocol(Method::GET, INSTALL_PATH, |state, request| {
            Box::pin(install(state, request))
        }),
        protocol(Method::POST, EVENTS_PATH, |state, request| {
            Box::pin(inbound::events(state, request))
        }),
        protocol(Method::POST, INTERACTIONS_PATH, |state, request| {
            Box::pin(inbound::interactions(state, request))
        }),
    ];
    // Import Files' own and Sync Canvases', each in a module of their own.
    for integration in own::ALL {
        routes.extend((integration.routes)());
    }
    routes
}

/// A folder's ID where the DSP keeps its files, as Google Drive writes them.
pub(crate) fn folder(id: &str) -> Result<String> {
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
/// A Slack workspace's ID, as Slack writes them.
pub(crate) fn team(id: &str) -> Result<String> {
    ensure(storage::team_id(id), "invalid_input", 400)?;
    Ok(id.to_owned())
}
/// A Slack channel's ID, as Slack writes them.
pub(crate) fn channel(id: &str) -> Result<String> {
    ensure(channels::channel_id(id), "invalid_input", 400)?;
    Ok(id.to_owned())
}
/// An integration's ID, as features declare them: `<feature>.<what>`.
fn integration(id: &str) -> Result<String> {
    ensure(
        (3..=64).contains(&id.len())
            && id.contains('.')
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.'),
        "invalid_input",
        400,
    )?;
    Ok(id.to_owned())
}
/// A DSP page's address, as its feature declares it: where an install started on it returns.
fn page(id: &str) -> Result<String> {
    ensure(
        (1..=40).contains(&id.len())
            && id.starts_with(|first: char| first.is_ascii_lowercase())
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid_input",
        400,
    )?;
    Ok(id.to_owned())
}
/// A workspace's name in Dispatch, as `body` gives it: left out or blank, it has none.
fn label(body: &Value) -> Result<Option<String>> {
    let label = optional_text(body, "label", 60)?.trim();
    Ok((!label.is_empty()).then(|| label.to_owned()))
}

fn account(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&connection::account(db, c.dsp_id())?)
}
/// A workspace's logo, the copy Dispatch keeps of it. Its address carries its version, `v`, so
/// the browser keeps it until the logo changes.
fn logo(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["v"])?;
    let team = team(input.param("team"))?;
    let (kind, picture) =
        logo::picture(db, c.dsp_id(), &team)?.ok_or_else(|| Error::new("not_found", 404))?;
    Ok(Reply::picture(kind, picture.into()))
}
/// Starts installing Dispatch in a workspace: a new one, with the name `label` in Dispatch, or
/// with `team`, one of the DSP's to reconnect or update. Slack sends the browser back to the
/// DSP's page `page`, where it started, or else to Settings' Connections.
fn connect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["team", "label", "page"])?;
    let asked = optional(&input.body, "team", |body, key| {
        team(v::text(body, key, 2, 32)?)
    })?;
    let label = label(&input.body)?;
    let back = optional(&input.body, "page", |body, key| {
        page(v::text(body, key, 1, 40)?)
    })?;
    Reply::of(&connection::start(db, c, &input.site, asked, label, back)?)
}
async fn finish(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["state", "code"])?;
    let sign_in = v::text(&input.body, "state", 40, 200)?.to_owned();
    let code = v::text(&input.body, "code", 1, 2048)?.to_owned();
    Reply::of(&connection::finish(&state, c, access, sign_in, code).await?)
}
fn rename(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["label"])?;
    let team = team(input.param("team"))?;
    Reply::of(&connection::rename(db, c, &team, label(&input.body)?)?)
}
async fn disconnect(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&connection::disconnect(&state, c, access, team).await?)
}
async fn list_channels(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&channels::list(&state, &c, &team).await?)
}
async fn join_channel(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let (team, channel) = (team(input.param("team"))?, channel(input.param("channel"))?);
    Reply::of(&channels::join(&state, c, access, team, channel).await?)
}
async fn leave_channel(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let (team, channel) = (team(input.param("team"))?, channel(input.param("channel"))?);
    Reply::of(&channels::leave(&state, c, access, team, channel).await?)
}
/// Switches one of the integrations through Slack on or off in the workspace `team`.
fn switch_integration(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["on"])?;
    let team = team(input.param("team"))?;
    let id = integration(input.param("integration"))?;
    let on = v::boolean(&input.body, "on")?;
    Reply::of(&integrations::switch(db, c, &team, &id, on)?)
}
fn create_link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["label"])?;
    Reply::of(&link::create(db, c, None, label(&input.body)?)?)
}
/// The install link for updating or reconnecting the workspace `team`.
fn create_workspace_link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&link::create(db, c, Some(&team), None)?)
}
fn cancel_link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&link::cancel(db, c)?)
}

/// What a request's query names, once each.
fn query(request: &Request) -> Vec<(String, String)> {
    url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .into_owned()
        .collect()
}
fn field<'a>(sent: &'a [(String, String)], name: &str) -> Option<&'a str> {
    sent.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// Slack sends the browser back here after installing, without the session: its cookie stays
/// on Dispatch's own address. An install a member started goes on, with what Slack sent, to
/// the DSP's page it started on, Settings' Connections unless it named another, at the address
/// they started at, which finishes it as them, in the session that started it. One the install
/// link started finishes here, as the link is all it needs, and goes on to Dispatch in Slack.
async fn returned(state: Arc<State>, request: Request) -> Response {
    let origin = state.config.origin.clone();
    let sent = query(&request);
    let sign_in = field(&sent, "state").unwrap_or("").to_owned();
    let Some((dsp, started)) = connection::started(&sign_in) else {
        return Reply::redirect(format!("{origin}/")).into_response();
    };
    let code = field(&sent, "code").filter(|code| (1..=2048).contains(&code.len()));
    if started == Started::Link {
        let Some(code) = code else {
            return pages::stopped("slack_cancelled");
        };
        return match link::finish(&state, sign_in.clone(), code.to_owned()).await {
            Ok((team, app)) => Reply::redirect(in_slack(&team, &app)).into_response(),
            Err(error) => pages::stopped(&error.code),
        };
    }
    let page = {
        let (at, asked) = (dsp.to_owned(), sign_in.clone());
        state
            .read(move |db| db.slack_install_page(&at, &asked))
            .await
            .ok()
            .flatten()
    };
    let back = {
        let mut back = url::form_urlencoded::Serializer::new(String::new());
        if page.is_none() {
            back.append_pair("tab", "connections");
        }
        back.append_pair("slackState", &sign_in);
        match code {
            Some(code) => back.append_pair("slackCode", code),
            None => back.append_pair("slackError", field(&sent, "error").unwrap_or("failed")),
        };
        back.finish()
    };
    // At the DSP's own address, its pages need no DSP named.
    let own = match started {
        Started::Own => {
            let at = dsp.to_owned();
            state
                .read(move |db| Ok(db.find_dsp(&at)?.code.map(|code| db.config.dsp_url(&code))))
                .await
                .ok()
                .flatten()
        }
        _ => None,
    };
    let page = page.as_deref().unwrap_or("settings");
    Reply::redirect(match own {
        Some(address) => format!("{address}/#{page}?{back}"),
        None => format!("{origin}/#dsp/{dsp}/{page}?{back}"),
    })
    .into_response()
}
/// Opens Dispatch in the workspace it was just added to, in Slack's app or its web.
fn in_slack(team: &str, app: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("app", app)
        .append_pair("team", team)
        .finish();
    format!("https://slack.com/app_redirect?{query}")
}

/// The install link: whoever opens it goes on to Slack, to add Dispatch to their workspace for
/// the DSP the link is for.
async fn install(state: Arc<State>, request: Request) -> Response {
    let sent = query(&request);
    let Some(opened) = field(&sent, "link")
        .filter(|opened| (1..=200).contains(&opened.len()))
        .map(str::to_owned)
    else {
        return pages::stopped("slack_link_expired");
    };
    // A link that doesn't work is turned away by a read, before anything takes the write lock.
    let checked = opened.clone();
    if let Err(error) = state.read(move |db| link::opened(db, &checked)).await {
        return pages::stopped(&error.code);
    }
    match state.run(move |db| link::start(db, &opened)).await {
        Ok(url) => Reply::redirect(url).into_response(),
        Err(error) => pages::stopped(&error.code),
    }
}
