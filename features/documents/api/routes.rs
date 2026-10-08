//! Documents's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs; the handlers that call Google wait
//! outside the database and check again before they write.
use crate::backend::{connection, google::RETURN_PATH};
use axum::{
    extract::Request,
    http::Method,
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Result, State,
    db::{Store, identifier},
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member, Route, async_post, protocol, read, write},
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
