use crate::{
    Result, State,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text, query_number},
        route::{Grant, PlatformRoutine, Route, Session, User, read, upload, write},
        upload::Upload,
    },
    support::{approvals, attachments, conversations, notifications, tickets},
};
use futures_util::{StreamExt, pin_mut};
use std::sync::Arc;

#[path = "worker_routes.rs"]
mod worker;
pub fn routes() -> Vec<Route> {
    let user = vec![
        read("/api/support/tickets", Session, mine),
        write("/api/support/tickets", Session, create),
        read("/api/support/tickets/{id}", Session, detail),
        write("/api/support/tickets/{id}/messages", Session, message),
        write("/api/support/tickets/{id}/reopen", Session, reopen),
        upload(
            "/api/support/attachments",
            Session,
            attachments::LIMIT,
            upload_image,
        ),
        read("/api/support/attachments/{id}", Session, image),
        read("/api/support/notifications", Session, notices),
        read("/api/support/notifications/unread", Session, unread),
        write("/api/support/notifications/read", Session, read_notices),
        read("/api/platform/support/tickets", PlatformRoutine, inbox),
        write(
            "/api/platform/support/tickets/{id}/decision",
            PlatformRoutine,
            decision,
        ),
        write(
            "/api/platform/support/tickets/{id}/resume",
            PlatformRoutine,
            resume,
        ),
    ];
    user.into_iter().chain(worker::routes()).collect()
}
fn listing(db: &crate::db::Store, user: &User, input: &Input, owner: bool) -> Result<Reply> {
    v::fields(&input.query, &["status", "before", "beforeId", "q"])?;
    let status = optional_text(&input.query, "status", 40)?;
    let before = query_number(
        &input.query,
        "before",
        i64::MAX as usize,
        0,
        i64::MAX as usize,
    )? as i64;
    Reply::of(&tickets::list(
        db,
        user,
        owner,
        status,
        before,
        optional_text(&input.query, "beforeId", 100)?,
        optional_text(&input.query, "q", 180)?,
    )?)
}
fn mine(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    listing(db, user, input, false)
}
fn inbox(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    listing(db, user, input, true)
}
fn detail(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    Reply::of(&tickets::detail(db, user, input.param("id"))?)
}
fn create(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    Reply::of_status(&tickets::create(db, user, &input.body)?, 201)
}
fn message(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["body", "target"])?;
    conversations::reply(
        db,
        user,
        input.param("id"),
        v::text(&input.body, "body", 1, 40_000)?,
        v::choice(&input.body, "target", &["reporter", "agent"])?,
    )?;
    detail(db, user, input)
}
fn reopen(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&tickets::reopen(db, user, input.param("id"))?)
}
fn decision(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["decision", "version", "note"])?;
    Reply::of(&approvals::decide(
        db,
        user,
        input.param("id"),
        v::integer(&input.body, "version", 1, u32::MAX.into())? as u32,
        v::choice(&input.body, "decision", &["approve", "deny"])?,
        optional_text(&input.body, "note", 40_000)?,
    )?)
}
fn resume(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&approvals::resume(db, user, input.param("id"))?)
}
fn notices(db: &crate::db::Store, user: &User, _: &Input) -> Result<Reply> {
    Reply::of(&notifications::list(db, user)?)
}
fn unread(db: &crate::db::Store, user: &User, _: &Input) -> Result<Reply> {
    Ok(Reply::json(
        serde_json::json!({"count":notifications::unread(db, user)?}),
    ))
}
fn read_notices(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["ids"])?;
    let ids: Vec<String> = serde_json::from_value(input.body["ids"].clone())?;
    notifications::read(db, user, &ids)?;
    Ok(Reply::ok())
}
fn image(db: &crate::db::Store, user: &User, input: &Input) -> Result<Reply> {
    let (kind, bytes) = attachments::read(db, user, input.param("id"))?;
    Ok(Reply::picture(&kind, bytes.into()))
}
pub(super) async fn bytes(upload: Upload) -> Result<Vec<u8>> {
    let stream = upload.stream();
    pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}
async fn upload_image(
    state: Arc<State>,
    input: Input,
    access: Session,
    upload: Upload,
) -> Result<Reply> {
    let request = input.clone();
    state.read(move |db| access.authorize(db, &request)).await?;
    let bytes = bytes(upload).await?;
    state
        .run(move |db| {
            let user = access.authorize(db, &input)?;
            v::fields(&input.query, &["name"])?;
            Reply::of_status(
                &attachments::save(db, &user, v::text(&input.query, "name", 1, 180)?, &bytes)?,
                201,
            )
        })
        .await
}
