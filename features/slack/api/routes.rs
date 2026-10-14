//! Slack's endpoints, each registered with its path and access. The server checks the session
//! and the permission before a handler runs; the handlers that call Slack wait outside the
//! database and check again before they write. Slack's own requests come to public addresses
//! outside `/api/`, each checked against Slack's signature before anything else.
use crate::backend::{
    app::{EVENTS_PATH, INSTALL_PATH, INTERACTIONS_PATH, RETURN_PATH},
    connection::{self, Started},
    inbound, link, pages, storage,
};
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::optional,
        input::{Input, Reply},
        route::{Dsp, Grant, Member, Route, async_post, protocol, read, write},
    },
};
use std::sync::Arc;

/// The DSP's Slack workspaces are among its own accounts, on Settings' DSP Connections.
const CONNECTIONS: Dsp = Dsp("connections.manage");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/slack/account", CONNECTIONS, account),
        write("/api/dsp/slack/connect", CONNECTIONS, connect),
        async_post("/api/dsp/slack/connect/finish", CONNECTIONS, finish),
        async_post(
            "/api/dsp/slack/workspaces/{team}/disconnect",
            CONNECTIONS,
            disconnect,
        ),
        write("/api/dsp/slack/link", CONNECTIONS, create_link),
        write("/api/dsp/slack/link/cancel", CONNECTIONS, cancel_link),
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
    ]
}

/// A Slack workspace's ID, as Slack writes them.
fn team(id: &str) -> Result<String> {
    ensure(storage::team_id(id), "invalid_input", 400)?;
    Ok(id.to_owned())
}

fn account(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&connection::account(db, c.dsp_id())?)
}
/// Starts installing Dispatch in a workspace: a new one, or with `team`, one of the DSP's to
/// reconnect or update.
fn connect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["team"])?;
    let asked = optional(&input.body, "team", |body, key| team(v::text(body, key, 2, 32)?))?;
    Reply::of(&connection::start(db, c, &input.site, asked)?)
}
async fn finish(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["state", "code"])?;
    let sign_in = v::text(&input.body, "state", 40, 200)?.to_owned();
    let code = v::text(&input.body, "code", 1, 2048)?.to_owned();
    Reply::of(&connection::finish(&state, c, access, sign_in, code).await?)
}
async fn disconnect(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let team = team(input.param("team"))?;
    Reply::of(&connection::disconnect(&state, c, access, team).await?)
}
fn create_link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&link::create(db, c)?)
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
/// Settings' Connections at the address they started at, which finishes it as them, in the
/// session that started it. One the install link started finishes here, as the link is all it
/// needs, and goes on to Dispatch in Slack.
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
    let back = {
        let mut back = url::form_urlencoded::Serializer::new(String::new());
        back.append_pair("tab", "connections");
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
    Reply::redirect(match own {
        Some(address) => format!("{address}/#settings?{back}"),
        None => format!("{origin}/#dsp/{dsp}/settings?{back}"),
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
    let Some(link) = field(&sent, "link")
        .filter(|link| (1..=200).contains(&link.len()))
        .map(str::to_owned)
    else {
        return pages::stopped("slack_link_expired");
    };
    match state
        .run(move |db| link::start(db, &link))
        .await
    {
        Ok(url) => Reply::redirect(url).into_response(),
        Err(error) => pages::stopped(&error.code),
    }
}
