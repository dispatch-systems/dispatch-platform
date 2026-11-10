//! Conduct's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs. Writing up, changing, signing and
//! deleting writeups is `conduct.use`; categories and how the backup is going are
//! `conduct.manage`.
use super::types::{CategoryInput, ConductSaved, WriteupInput};
use crate::backend::{DOMAIN, backups, categories, copies, overview, writeups};
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, query_number},
        route::{Dsp, Grant, Member, Route, async_get, read, upload, write},
        upload::Upload,
    },
};
use futures_util::{StreamExt, pin_mut};
use std::sync::Arc;

const USE: Dsp = Dsp("conduct.use");
const MANAGE: Dsp = Dsp("conduct.manage");

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/conduct", USE, overview_read),
        write("/api/dsp/conduct/writeups", USE, writeup_add),
        read("/api/dsp/conduct/writeups/{id}", USE, writeup_read),
        write("/api/dsp/conduct/writeups/{id}", USE, writeup_change),
        write("/api/dsp/conduct/writeups/{id}/sign", USE, writeup_sign),
        write("/api/dsp/conduct/writeups/{id}/delete", USE, writeup_delete),
        upload(
            "/api/dsp/conduct/writeups/{id}/copy",
            USE,
            copies::LIMIT,
            copy_keep,
        ),
        read("/api/dsp/conduct/writeups/{id}/copy", USE, copy_read),
        write("/api/dsp/conduct/categories", MANAGE, category_add),
        write("/api/dsp/conduct/categories/{id}", MANAGE, category_change),
        write(
            "/api/dsp/conduct/categories/{id}/retire",
            MANAGE,
            category_retire,
        ),
        write(
            "/api/dsp/conduct/categories/{id}/restore",
            MANAGE,
            category_restore,
        ),
        async_get("/api/dsp/conduct/backup", MANAGE, backup_read),
    ]
}

fn overview_read(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    Reply::of(&overview::read(db, c.state, c)?)
}
/// The writeup `id` as it is now, with the page's data.
fn saved(db: &Store, c: &Member, id: &str) -> Result<ConductSaved> {
    Ok(ConductSaved {
        writeup: overview::writeup(db, c.state, &c.dsp.id, id)?,
        overview: overview::read(db, c.state, c)?,
    })
}

fn writeup_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    Reply::of(&overview::writeup(
        db,
        c.state,
        &c.dsp.id,
        input.param("id"),
    )?)
}
fn writeup_add(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let writing = WriteupInput::parse(&input.body, false)?;
    let subject = overview::subject(db, c.state, &c.dsp.id, &writing.person)?;
    let id = writeups::add(db, c, &subject, &writing, &overview::today(c)?)?;
    Reply::of_status(&saved(db, c, &id)?, 201)
}
fn writeup_change(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let id = input.param("id");
    let writing = WriteupInput::parse(&input.body, true)?;
    let subject = overview::subject(db, c.state, &c.dsp.id, &writing.person)?;
    writeups::change(db, c, id, &subject, &writing, &overview::today(c)?)?;
    Reply::of(&saved(db, c, id)?)
}
fn writeup_sign(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["signedOn", "revision"])?;
    let signed_on = v::text(&input.body, "signedOn", 10, 10)?;
    v::date(signed_on)?;
    let id = input.param("id");
    let revision = v::integer(&input.body, "revision", 1, i64::MAX)?;
    writeups::sign(db, c, id, revision, signed_on, &overview::today(c)?)?;
    Reply::of(&saved(db, c, id)?)
}
fn writeup_delete(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["revision"])?;
    let revision = v::integer(&input.body, "revision", 1, i64::MAX)?;
    writeups::delete(db, c, input.param("id"), revision)?;
    Reply::of(&overview::read(db, c.state, c)?)
}

/// Keeps a photo of a writeup's signed paper, its bytes read whole before the write.
async fn copy_keep(state: Arc<State>, input: Input, access: Dsp, upload: Upload) -> Result<Reply> {
    v::fields(&input.query, &["revision"])?;
    let revision = i64::try_from(query_number(&input.query, "revision", 0, 1, usize::MAX)?)
        .map_err(|_| dispatch_core::Error::new("invalid_input", 400))?;
    let asked = input.clone();
    let dsp = state
        .read(move |db| Ok(access.authorize(db, &asked)?.dsp.id))
        .await?;
    let stream = upload.stream();
    pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    let ran = state.clone();
    state
        .run_scoped(dsp, DOMAIN, move |db| {
            // Checked again: the member may have lost access while the photo came in.
            let c = access.authorize(db, &input)?;
            let id = input.param("id");
            copies::keep(db, &c, id, revision, &bytes)?;
            Reply::of(&ConductSaved {
                writeup: overview::writeup(db, &ran, &c.dsp.id, id)?,
                overview: overview::read(db, &ran, &c)?,
            })
        })
        .await
}
fn copy_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let (kind, bytes) = copies::read(db, &c.dsp.id, input.param("id"))?;
    Ok(Reply::picture(&kind, bytes.into()))
}

fn category_add(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    categories::add(db, c, &CategoryInput::parse(&input.body, false)?)?;
    Reply::of_status(&overview::read(db, c.state, c)?, 201)
}
fn category_change(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let setting = CategoryInput::parse(&input.body, true)?;
    categories::change(db, c, input.param("id"), &setting)?;
    Reply::of(&overview::read(db, c.state, c)?)
}
fn category_retire(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    categories::retire(db, c, input.param("id"), false)?;
    Reply::of(&overview::read(db, c.state, c)?)
}
fn category_restore(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    categories::retire(db, c, input.param("id"), true)?;
    Reply::of(&overview::read(db, c.state, c)?)
}

/// How backing signed copies up is going, which asks what keeps the DSP's files whether it can:
/// answered only to a member who may still read it once that's asked.
async fn backup_read(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let c = state.read(move |db| access.authorize(db, &input)).await?;
    let status = backups::status(&state, &c.dsp.id).await?;
    state
        .read(move |db| access.revalidate(db, &c).map(drop))
        .await?;
    Reply::of(&status)
}
