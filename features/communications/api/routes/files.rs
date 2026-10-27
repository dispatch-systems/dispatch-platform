//! Photos and files: one added for a Slack message not sent yet, one attached to a message as
//! kept, a workspace's logo, and the public image of someone's initials.
use super::{id, slack_id};
use crate::backend::{
    conversations,
    files::{self, FileStatus, FileStore},
    initials, reply,
};
use axum::{
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply},
        route::{Dsp, Grant, Member},
        upload::Upload,
    },
};
use std::sync::Arc;

/// Keeps a photo added for a message not sent yet: its `name` and `type` in the query.
pub(super) async fn add_photo(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    photo: Upload,
) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["name", "type"])?;
    let name = v::name(&input.query, "name", 200)?;
    let kind = v::text(&input.query, "type", 1, 100)?;
    let added = reply::upload(&state, &c, access, &name, kind, photo).await?;
    Reply::of_status(&added, 201)
}
/// A photo or file attached to a message, as kept. One added for a message not sent yet is its
/// adder's alone.
pub(super) async fn file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let wanted = id(input.param("id"), "file")?;
    let (c, file) = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            let file = db.communications_file(&c.dsp.id, &wanted)?;
            Ok((c, file))
        })
        .await?;
    v::fields(&input.query, &[])?;
    let file = file
        .filter(|file| file.status == FileStatus::Kept)
        .filter(|file| {
            file.message.is_some()
                || file.group_message.is_some()
                || file.uploaded_by.as_ref() == Some(&c.auth.user.id)
        })
        .ok_or_else(|| Error::new("communications_file_not_found", 404))?;
    let sha256 = file.sha256.unwrap_or_default();
    let bytes = files::read(&state, &c.dsp.id, &sha256).await?;
    Ok(Reply::download(
        &file.mime,
        &file.name,
        Some(file.size as u64),
        Body::from_stream(bytes),
    ))
}
/// A workspace's logo, as Slack keeps a copy of it. Its address carries its version, `v`, so the
/// browser keeps it until the logo changes.
pub(super) fn logo(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["v"])?;
    let team = slack_id(input.param("team"), b"T")?;
    let (kind, picture) =
        conversations::logo(db, c.dsp_id(), &team)?.ok_or_else(|| Error::new("not_found", 404))?;
    Ok(Reply::picture(kind, picture.into()))
}

/// The image of someone's initials, which Slack shows their messages under: public, as Slack
/// fetches it from wherever its link points, and showing two letters and nothing else.
pub(super) fn picture(request: Request) -> Response {
    let name = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    match initials::image(&name) {
        Some(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            Body::from((*png).clone()),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
