//! Documents's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs; the handlers that call Google wait
//! outside the database and check again before they write.
use crate::{
    api::types::NewKind,
    backend::{connection, files, google::RETURN_PATH, team},
};
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Result, State,
    db::{Store, identifier},
    ensure,
    foundation::validate as v,
    server::http::{
        input::optional,
        input::{Input, Reply},
        route::{Dsp, Grant, Member, Route, async_get, async_post, protocol, read, write},
    },
};
use std::sync::Arc;

const USE: Dsp = Dsp("documents.use");
const MANAGE: Dsp = Dsp("documents.manage");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/documents", USE, overview),
        write("/api/dsp/documents/connect", MANAGE, connect),
        async_post("/api/dsp/documents/connect/finish", MANAGE, finish),
        async_post("/api/dsp/documents/disconnect", MANAGE, disconnect),
        async_get("/api/dsp/documents/folder", USE, folder),
        async_post("/api/dsp/documents/new", USE, create),
        async_post("/api/dsp/documents/items/{id}/rename", USE, rename),
        async_post("/api/dsp/documents/items/{id}/trash", USE, trash),
        write("/api/dsp/documents/link", USE, link),
        async_post("/api/dsp/documents/link/finish", USE, finish_link),
        async_get("/api/dsp/documents/team", MANAGE, team_view),
        async_post("/api/dsp/documents/team/email", MANAGE, email_again),
        async_post("/api/dsp/documents/team/remove", MANAGE, remove_share),
        protocol(Method::GET, RETURN_PATH, |state, request| {
            Box::pin(returned(state, request))
        }),
    ]
}

fn overview(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&connection::overview(db, c)?)
}
fn connect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&connection::start(db, c)?)
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
    Reply::of(&connection::disconnect(&state, c, access).await?)
}

/// A Drive file's ID, as Google writes them.
fn file_id(id: &str) -> Result<String> {
    let valid = (1..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ensure(valid, "invalid_input", 400)?;
    Ok(id.to_owned())
}
/// A file's name: up to 200 characters, without control characters or space around it.
fn file_name(body: &serde_json::Value) -> Result<String> {
    let name = v::text(body, "name", 1, 400)?.trim();
    let valid = (1..=200).contains(&name.chars().count()) && !name.chars().any(char::is_control);
    ensure(valid, "documents_name_invalid", 400)?;
    Ok(name.to_owned())
}

async fn folder(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["id", "q"])?;
    let id = optional(&input.query, "id", |q, key| {
        file_id(v::text(q, key, 1, 128)?)
    })?;
    let query = optional(&input.query, "q", |q, key| {
        Ok(v::text(q, key, 0, 100)?.to_owned())
    })?;
    Reply::of(&files::folder(&state, &c, id, query).await?)
}
async fn create(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["folder", "kind", "name"])?;
    let folder = match &input.body["folder"] {
        serde_json::Value::Null => None,
        _ => Some(file_id(v::text(&input.body, "folder", 1, 128)?)?),
    };
    let kind = NewKind::parse(v::choice(
        &input.body,
        "kind",
        &["folder", "doc", "sheet", "slides"],
    )?)
    .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?;
    let name = file_name(&input.body)?;
    Reply::of(&files::create(&state, c, access, folder, kind, name).await?)
}
async fn rename(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["name"])?;
    let (id, name) = (file_id(input.param("id"))?, file_name(&input.body)?);
    Reply::of(&files::rename(&state, c, access, id, name).await?)
}
async fn trash(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    files::trash(&state, c, access, file_id(input.param("id"))?).await?;
    Ok(Reply::ok())
}

fn link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&team::start_link(db, c)?)
}
async fn finish_link(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["state", "code"])?;
    let sign_in = v::text(&input.body, "state", 40, 200)?.to_owned();
    let code = v::text(&input.body, "code", 1, 2048)?.to_owned();
    Reply::of(&team::finish_link(&state, c, access, sign_in, code).await?)
}
async fn team_view(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let c = state.run(move |db| access.authorize(db, &input)).await?;
    Reply::of(&team::team(&state, &c).await?)
}
async fn email_again(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["user"])?;
    let user = v::text(&input.body, "user", 1, 100)?.to_owned();
    Reply::of(&team::email_again(&state, c, access, user).await?)
}
async fn remove_share(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["share"])?;
    let share = file_id(v::text(&input.body, "share", 1, 128)?)?;
    Reply::of(&team::remove(&state, c, access, share).await?)
}

/// Google sends the browser back here, without the session: its cookie stays on Dispatch's
/// own site. So the browser goes on to the DSP's Documents page with what Google sent, and the
/// page finishes the sign-in as the member, in the session that started it.
async fn returned(state: Arc<State>, request: Request) -> Response {
    let origin = &state.config.origin;
    let sent: Vec<(String, String)> =
        url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    let field = |name: &str| {
        sent.iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.as_str())
    };
    let sign_in = field("state").unwrap_or("");
    let Some(dsp) = sign_in
        .split_once('.')
        .map(|(dsp, _)| dsp)
        .filter(|dsp| identifier(dsp, "dsp_"))
    else {
        return Reply::redirect(format!("{origin}/")).into_response();
    };
    let mut back = url::form_urlencoded::Serializer::new(String::new());
    back.append_pair("googleState", sign_in);
    match field("code") {
        Some(code) => back.append_pair("googleCode", code),
        None => back.append_pair("googleError", field("error").unwrap_or("failed")),
    };
    Reply::redirect(format!("{origin}/#dsp/{dsp}/documents?{}", back.finish())).into_response()
}
