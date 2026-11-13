//! Conduct's endpoints, each registered with its path and access. The server checks the
//! session and the permission before a handler runs. Writing up, changing, signing and
//! deleting writeups is `conduct.use`, deleting a signed one `conduct.manage`; categories and
//! how the backup is going are `conduct.manage`.
use super::types::{CategoryInput, ConductSaved, WriteupInput};
use crate::backend::{DOMAIN, backups, categories, copies, history, overview, writeups};
use dispatch_core::{
    Result, State,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional, optional_text, query_number},
        route::{
            Dsp, Grant, Member, Route, async_get, async_post, coalesced_read, read, upload, write,
        },
        upload::Upload,
    },
};
use futures_util::{StreamExt, pin_mut};
use std::sync::Arc;

const USE: Dsp = Dsp("conduct.use");
const MANAGE: Dsp = Dsp("conduct.manage");

pub fn routes() -> Vec<Route> {
    vec![
        coalesced_read("/api/dsp/conduct", USE, overview_read),
        read("/api/dsp/conduct/writeups", USE, history_read),
        read("/api/dsp/conduct/context", USE, context_read),
        read(
            "/api/dsp/conduct/categories/{id}/impact",
            MANAGE,
            impact_read,
        ),
        write("/api/dsp/conduct/writeups", USE, writeup_add),
        read("/api/dsp/conduct/writeups/{id}", USE, writeup_read),
        write("/api/dsp/conduct/writeups/{id}", USE, writeup_change),
        write("/api/dsp/conduct/writeups/{id}/sign", USE, writeup_sign),
        async_post("/api/dsp/conduct/writeups/{id}/delete", USE, writeup_delete),
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

fn overview_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let bytes = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::backend::CACHED, &c.dsp.id),
        overview::today(c)?,
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || overview::read(db, c.state, c),
    )?;
    Ok(Reply::encoded(bytes))
}
fn history_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(
        q,
        &[
            "person", "q", "category", "tier", "status", "after", "limit",
        ],
    )?;
    let tier = optional(q, "tier", |q, k| {
        v::choice(q, k, &["", "verbal", "written", "final"])
    })?
    .unwrap_or("");
    let status = optional(q, "status", |q, k| {
        v::choice(q, k, &["", "waiting", "signed"])
    })?
    .unwrap_or("");
    Reply::of(&history::read(
        db,
        c.state,
        &c.dsp.id,
        history::Query {
            person: optional_text(q, "person", 128)?,
            search: optional_text(q, "q", 200)?,
            category: optional_text(q, "category", 64)?,
            tier,
            status,
            after: optional_text(q, "after", 256)?,
            limit: query_number(q, "limit", 50, 1, 100)?,
        },
    )?)
}
fn context_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let q = &input.query;
    v::fields(q, &["person", "category", "date", "edit"])?;
    let date = v::text(q, "date", 10, 10)?;
    v::date(date)?;
    Reply::of(&history::context(
        db,
        c.state,
        &c.dsp.id,
        v::text(q, "person", 1, 128)?,
        v::text(q, "category", 1, 64)?,
        date,
        optional_text(q, "edit", 128)?,
    )?)
}
fn impact_read(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    use super::types::{ConductStepDown, ConductStepKind, ConductUnit};
    let q = &input.query;
    v::fields(q, &["kind", "after", "unit", "revision"])?;
    let kind = v::choice(q, "kind", &["reduce", "reset", "never"])?;
    let step = if kind == "never" {
        None
    } else {
        Some(ConductStepDown {
            kind: ConductStepKind::parse(kind)
                .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?,
            after: query_number(q, "after", 6, 1, 999)? as i64,
            unit: ConductUnit::parse(v::choice(q, "unit", &["days", "weeks", "months"])?)
                .ok_or_else(|| dispatch_core::Error::new("invalid_input", 400))?,
        })
    };
    Reply::of(&history::impact(
        db,
        c.state,
        c,
        input.param("id"),
        query_number(q, "revision", 0, 1, i64::MAX as usize)? as i64,
        step,
    )?)
}
/// The writeup `id` as it is now, without repeating the page's history.
fn saved(db: &Store, c: &Member, id: &str) -> Result<ConductSaved> {
    Ok(ConductSaved {
        writeup: overview::writeup(db, c.state, &c.dsp.id, id)?,
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
    v::fields(&input.body, &["signedOn", "revision", "request"])?;
    let signed_on = v::text(&input.body, "signedOn", 10, 10)?;
    v::date(signed_on)?;
    let id = input.param("id");
    let revision = v::integer(&input.body, "revision", 1, i64::MAX)?;
    let request = optional(&input.body, "request", |b, k| v::text(b, k, 16, 100))?;
    if request.is_some() {
        writeups::sign_requested(
            db,
            c,
            id,
            revision,
            signed_on,
            &overview::today(c)?,
            request,
        )?;
    } else {
        writeups::sign(db, c, id, revision, signed_on, &overview::today(c)?)?;
    }
    Reply::of(&saved(db, c, id)?)
}
/// Deletes a writeup, a signed one only for a member who manages Conduct. Its copy where the
/// DSP keeps its files goes first, outside the database, while the DSP backs its copies up:
/// the writeup stays until that copy is gone.
async fn writeup_delete(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    v::fields(&input.body, &["revision"])?;
    let revision = v::integer(&input.body, "revision", 1, i64::MAX)?;
    let asked = input.clone();
    let (c, copy) = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            let copy = backups::saved(db, &c, asked.param("id"), revision)?;
            Ok((c, copy))
        })
        .await?;
    if let Some((files, saved)) = copy {
        backups::take_back(&state, &c, files, &saved).await?;
    }
    let ran = state.clone();
    state
        .run_scoped(c.dsp.id.clone(), DOMAIN, move |db| {
            // Checked again: the member may have lost access while the copy went.
            let c = access.authorize(db, &input)?;
            writeups::delete(db, &c, input.param("id"), revision)?;
            Reply::of(&overview::read(db, &ran, &c)?)
        })
        .await
}

/// Keeps a photo of a writeup's signed paper, its bytes read whole before the write.
async fn copy_keep(state: Arc<State>, input: Input, access: Dsp, upload: Upload) -> Result<Reply> {
    v::fields(&input.query, &["revision", "request"])?;
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
            let request = optional(&input.query, "request", |q, k| v::text(q, k, 16, 100))?;
            if request.is_some() {
                copies::keep_requested(db, &c, id, revision, &bytes, request)?;
            } else {
                copies::keep(db, &c, id, revision, &bytes)?;
            }
            Reply::of(&ConductSaved {
                writeup: overview::writeup(db, &ran, &c.dsp.id, id)?,
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
