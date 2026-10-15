//! Documents's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs; the handlers that call Google wait
//! outside the database and check again before they write.
use crate::{
    api::types::{NewKind, SharingState},
    backend::{
        connection,
        drive::PICTURE_SIZES,
        files,
        google::{self, RETURN_PATH},
        picker, team,
    },
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
        route::{Dsp, Grant, Member, Route, async_get, async_post, page, protocol, upload, write},
        upload::{UPLOAD_LIMIT, Upload},
    },
};
use std::sync::Arc;

const USE: Dsp = Dsp("documents.use");
/// The DSP's Google account is one of its own accounts on Settings' DSP Connections.
const CONNECTIONS: Dsp = Dsp("connections.manage");

pub fn routes() -> Vec<Route> {
    vec![
        async_get("/api/dsp/documents", USE, overview),
        async_get("/api/dsp/documents/account", CONNECTIONS, account),
        write("/api/dsp/documents/connect", CONNECTIONS, connect),
        async_post("/api/dsp/documents/connect/finish", CONNECTIONS, finish),
        async_post("/api/dsp/documents/disconnect", CONNECTIONS, disconnect),
        async_get("/api/dsp/documents/folder", USE, folder),
        async_post("/api/dsp/documents/new", USE, create),
        upload("/api/dsp/documents/upload", USE, UPLOAD_LIMIT, upload_file),
        async_get("/api/dsp/documents/items/{id}/download", USE, download),
        async_get("/api/dsp/documents/items/{id}/thumbnail", USE, thumbnail),
        async_post("/api/dsp/documents/add", USE, add_files),
        async_post("/api/dsp/documents/items/{id}/rename", USE, rename),
        async_post("/api/dsp/documents/items/{id}/move", USE, move_item),
        async_post("/api/dsp/documents/items/{id}/trash", USE, trash),
        write("/api/dsp/documents/link", USE, link),
        async_post("/api/dsp/documents/link/finish", USE, finish_link),
        protocol(Method::GET, RETURN_PATH, |state, request| {
            Box::pin(returned(state, request))
        }),
        page(picker::PICKER_PATH, |state, request| {
            Box::pin(async move { picker::page(&state.config, request.extensions().get()) })
        }),
    ]
}

/// The connection as the member sees it. One Documents hasn't shared the folder with yet has
/// it shared now, rather than within the minute.
async fn overview(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let c = state.run(move |db| access.authorize(db, &input)).await?;
    let dsp = c.dsp.id.clone();
    let answer = state.read(move |db| connection::overview(db, &c)).await?;
    let pending = answer
        .me
        .as_ref()
        .is_some_and(|me| me.state == SharingState::Pending);
    if pending && answer.connection.is_some() {
        team::nudge(&state, &dsp);
    }
    Reply::of(&answer)
}
async fn account(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let c = state.run(move |db| access.authorize(db, &input)).await?;
    Reply::of(&connection::account(&state, &c.dsp.id).await?)
}
fn connect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&connection::start(db, c, &input.site)?)
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
/// The folder a body names, `folder`: none for the main one.
fn into_folder(body: &serde_json::Value) -> Result<Option<String>> {
    match &body["folder"] {
        serde_json::Value::Null => Ok(None),
        _ => Ok(Some(file_id(v::text(body, "folder", 1, 128)?)?)),
    }
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
    let folder = into_folder(&input.body)?;
    let kind = NewKind::parse(v::choice(
        &input.body,
        "kind",
        &["folder", "doc", "sheet", "slides"],
    )?)
    .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?;
    let name = file_name(&input.body)?;
    Reply::of(&files::create(&state, c, access, folder, kind, name).await?)
}
/// A file's type as the browser named it, or bytes of no type it knows. Never one of
/// Google's own, which Google would try to make of the bytes.
fn file_type(query: &serde_json::Value) -> Result<String> {
    let Some(kind) = optional(query, "type", |q, key| v::text(q, key, 0, 120))? else {
        return Ok("application/octet-stream".to_owned());
    };
    let named = kind.split_once('/').is_some_and(|(kind, sub)| {
        [kind, sub].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
        })
    });
    Ok(
        if named && !kind.starts_with("application/vnd.google-apps.") {
            kind.to_ascii_lowercase()
        } else {
            "application/octet-stream".to_owned()
        },
    )
}
async fn upload_file(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    upload: Upload,
) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["folder", "name", "type"])?;
    let folder = optional(&input.query, "folder", |q, key| {
        file_id(v::text(q, key, 1, 128)?)
    })?;
    let name = file_name(&input.query)?;
    let kind = file_type(&input.query)?;
    Reply::of(&files::upload(&state, c, access, folder, name, kind, upload).await?)
}
/// The most files one pick adds.
const PICKED: usize = 50;
async fn add_files(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["files", "folder"])?;
    let folder = into_folder(&input.body)?;
    let files = input.body["files"]
        .as_array()
        .filter(|files| (1..=PICKED).contains(&files.len()))
        .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?
        .iter()
        .map(|file| file_id(file.as_str().unwrap_or("")))
        .collect::<Result<Vec<_>>>()?;
    Reply::of(&files::add(&state, c, access, files, folder).await?)
}
async fn download(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let id = file_id(input.param("id"))?;
    let file = files::download(&state, &c, id).await?;
    Ok(Reply::download(
        &file.kind,
        &file.name,
        file.length,
        file.body,
    ))
}
/// Google's picture of a file. Its address names the picture's version, `v`, which only tells
/// one picture from the next, and its size, `s`, for the screen: 600 pixels unless it says.
async fn thumbnail(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["v", "s"])?;
    let id = file_id(input.param("id"))?;
    let size = optional(&input.query, "s", |q, key| {
        let size = v::text(q, key, 1, 4)?.parse::<u32>().unwrap_or(0);
        ensure(PICTURE_SIZES.contains(&size), "invalid_input", 400)?;
        Ok(size)
    })?;
    let picture = files::picture(&state, &c, id, size.unwrap_or(PICTURE_SIZES[1])).await?;
    // The member's access may have changed while Google answered.
    state.read(move |db| access.revalidate(db, &c)).await?;
    Ok(Reply::picture(&picture.kind, picture.bytes))
}
async fn rename(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["name"])?;
    let (id, name) = (file_id(input.param("id"))?, file_name(&input.body)?);
    Reply::of(&files::rename(&state, c, access, id, name).await?)
}
async fn move_item(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["folder"])?;
    let (id, folder) = (file_id(input.param("id"))?, into_folder(&input.body)?);
    Reply::of(&files::move_to(&state, c, access, id, folder).await?)
}
async fn trash(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    files::trash(&state, c, access, file_id(input.param("id"))?).await?;
    Ok(Reply::ok())
}

/// Starts linking the member's own Google account: from Settings' Connections when `from`
/// says so, or else from Documents, where Google sends them back.
fn link(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["from"])?;
    let from = optional(&input.body, "from", |b, key| {
        v::choice(b, key, &["settings"])
    })?;
    Reply::of(&team::start_link(db, c, &input.site, from.is_some())?)
}
async fn finish_link(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.run(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["state", "code"])?;
    let sign_in = v::text(&input.body, "state", 40, 200)?.to_owned();
    let code = v::text(&input.body, "code", 1, 2048)?.to_owned();
    Reply::of(&team::finish_link(&state, c, access, sign_in, code).await?)
}
/// Google sends the browser back here, without the session: its cookie stays on Dispatch's
/// own site. So the browser goes on with what Google sent to the page the sign-in started on,
/// the DSP's Connections or Documents, at the address it started at, the admin's or the DSP's
/// own, which finishes it as the member, in the session that started it.
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
    // Google's answer to the picker's page is in the fragment, which only the browser reads.
    if sent.is_empty() {
        return picker::relay(&state.config);
    }
    let sign_in = field("state").unwrap_or("");
    let Some(dsp) = sign_in
        .split_once('.')
        .map(|(dsp, _)| dsp)
        .filter(|dsp| identifier(dsp, "dsp_"))
    else {
        return Reply::redirect(format!("{origin}/")).into_response();
    };
    let (page, back) = {
        let mut back = url::form_urlencoded::Serializer::new(String::new());
        let page = if team::back_to_documents(sign_in) {
            "documents"
        } else {
            back.append_pair("tab", "connections");
            "settings"
        };
        back.append_pair("googleState", sign_in);
        match field("code") {
            Some(code) => back.append_pair("googleCode", code),
            None => back.append_pair("googleError", field("error").unwrap_or("failed")),
        };
        (page, back.finish())
    };
    // At the DSP's own address, where the member's session is, its pages need no DSP named.
    let own = if google::started_at_own_address(sign_in) {
        let at = dsp.to_owned();
        state
            .read(move |db| Ok(db.find_dsp(&at)?.code.map(|code| db.config.dsp_url(&code))))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    Reply::redirect(match own {
        Some(address) => format!("{address}/#{page}?{back}"),
        None => format!("{origin}/#dsp/{dsp}/{page}?{back}"),
    })
    .into_response()
}
