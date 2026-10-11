//! Trusted adapters hold these credentials; Hermes tools receive only scoped job capabilities.
use crate::{
    Result, State,
    db::Store,
    ensure,
    foundation::{config::Site, crypto, validate as v},
    server::http::{
        input::{Input, Reply},
        route::{Access, Ctx, Grant, Route, read, upload, write},
        upload::Upload,
    },
    support::{
        attachments, browser, diagnostics, github_events, jobs, proposals, tickets, verification,
    },
};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Copy)]
struct Worker(bool);
impl Grant for Worker {
    type Who = ();
    fn access(self) -> Access {
        Access::Agent(if self.0 {
            "support github"
        } else {
            "support worker"
        })
    }
    fn authorize(self, db: &Store, input: &Input) -> Result<()> {
        ensure(
            input.site == Site::Admin && !input.headers.contains_key("cookie"),
            "sign_in_required",
            401,
        )?;
        let key = if self.0 {
            "SUPPORT_GITHUB_TOKEN"
        } else {
            "SUPPORT_WORKER_TOKEN"
        };
        let expected = db
            .config
            .setting(key)
            .ok_or_else(|| crate::Error::new("support_worker_unavailable", 503))?;
        let bearer = input
            .header("authorization")
            .strip_prefix("Bearer ")
            .unwrap_or("");
        ensure(
            expected.len() >= 32
                && input.headers.get_all("authorization").iter().count() == 1
                && crypto::equal(&crypto::sha(&expected), &crypto::sha(bearer)),
            "sign_in_required",
            401,
        )
    }
}
pub(super) fn routes() -> Vec<Route> {
    vec![
        write("/api/v1/support/worker/claim", Worker(false), claim),
        read(
            "/api/v1/support/worker/jobs/{id}/status",
            Worker(false),
            status,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/heartbeat",
            Worker(false),
            heartbeat,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/progress",
            Worker(false),
            progress,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/proposal",
            Worker(false),
            proposal,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/evidence",
            Worker(false),
            evidence,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/github",
            Worker(false),
            github,
        ),
        write(
            "/api/v1/support/worker/jobs/{id}/complete",
            Worker(false),
            complete,
        ),
        write("/api/v1/support/worker/jobs/{id}/wait", Worker(false), wait),
        write(
            "/api/v1/support/worker/jobs/{id}/browser",
            Worker(false),
            browser,
        ),
        read(
            "/api/v1/support/worker/jobs/{id}/diagnostics",
            Worker(false),
            diagnostics,
        ),
        upload(
            "/api/v1/support/worker/jobs/{id}/attachments",
            Worker(false),
            attachments::LIMIT,
            upload_image,
        ),
        read(
            "/api/v1/support/worker/jobs/{id}/attachments/{image}",
            Worker(false),
            image,
        ),
        write("/api/v1/support/worker/github/events", Worker(true), event),
    ]
}
fn lease(input: &Input) -> Result<&str> {
    v::text(&input.body, "lease", 43, 43)
}
fn claim(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    Reply::of(&jobs::claim(db)?)
}
fn status(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    Ok(Reply::json(jobs::status(
        db,
        input.param("id"),
        input.header("x-support-lease"),
    )?))
}
fn heartbeat(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "checkpoint"])?;
    let until = jobs::heartbeat(
        db,
        input.param("id"),
        lease(input)?,
        &input.body["checkpoint"],
    )?;
    Ok(Reply::json(json!({"ok":true,"leaseUntil":until})))
}
fn progress(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "status", "progress", "checkpoint"])?;
    jobs::progress(
        db,
        input.param("id"),
        lease(input)?,
        v::text(&input.body, "status", 1, 40)?,
        v::text(&input.body, "progress", 1, 1000)?,
        &input.body["checkpoint"],
    )?;
    Ok(Reply::ok())
}
fn proposal(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "proposal"])?;
    let run = jobs::leased(db, input.param("id"), lease(input)?)?;
    proposals::publish(
        db,
        input.param("id"),
        lease(input)?,
        serde_json::from_value(input.body["proposal"].clone())?,
    )?;
    Reply::of(&tickets::view(db, &run.ticket, true)?)
}
fn evidence(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "evidence"])?;
    Reply::of(&verification::record(
        db,
        input.param("id"),
        lease(input)?,
        serde_json::from_value(input.body["evidence"].clone())?,
    )?)
}
fn github(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "prUrl", "headSha"])?;
    verification::github(
        db,
        input.param("id"),
        lease(input)?,
        v::text(&input.body, "prUrl", 1, 256)?,
        v::text(&input.body, "headSha", 40, 40)?,
    )?;
    Ok(Reply::ok())
}
fn complete(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease", "result"])?;
    verification::complete(db, input.param("id"), lease(input)?, &input.body["result"])?;
    Ok(Reply::ok())
}
fn wait(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(
        &input.body,
        &["lease", "reason", "checkpoint", "retryAfterMs"],
    )?;
    let delay = input
        .body
        .get("retryAfterMs")
        .map(|_| v::integer(&input.body, "retryAfterMs", 1, i64::MAX / 2))
        .transpose()?;
    jobs::wait(
        db,
        input.param("id"),
        lease(input)?,
        v::text(&input.body, "reason", 1, 1000)?,
        &input.body["checkpoint"],
        delay,
    )?;
    Ok(Reply::ok())
}
fn browser(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["lease"])?;
    Ok(Reply::json(browser::issue(
        db,
        input.param("id"),
        lease(input)?,
    )?))
}
fn diagnostics(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    let run = jobs::leased(db, input.param("id"), input.header("x-support-lease"))?;
    Ok(Reply::json(diagnostics::worker(db, &run.ticket)?))
}
fn image(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    let run = jobs::leased(db, input.param("id"), input.header("x-support-lease"))?;
    ensure(
        db.platform.count(
            "SELECT count(*) FROM support_attachments WHERE id=? AND ticket_id=?",
            rusqlite::params![input.param("image"), run.ticket],
        )? == 1,
        "support_ticket_not_found",
        404,
    )?;
    let (kind, bytes) = attachments::bytes(db, input.param("image"))?;
    Ok(Reply::picture(&kind, bytes.into()))
}
async fn upload_image(
    state: Arc<State>,
    input: Input,
    access: Worker,
    upload: Upload,
) -> Result<Reply> {
    let request = input.clone();
    state
        .read(move |db| {
            access.authorize(db, &request)?;
            jobs::leased(db, request.param("id"), request.header("x-support-lease"))?;
            Ok(())
        })
        .await?;
    let bytes = super::bytes(upload).await?;
    state
        .run(move |db| {
            access.authorize(db, &input)?;
            let run = jobs::leased(db, input.param("id"), input.header("x-support-lease"))?;
            v::fields(&input.query, &["name"])?;
            Reply::of(&attachments::store(
                db,
                input.param("id"),
                "worker",
                Some(&run.ticket),
                true,
                v::text(&input.query, "name", 1, 180)?,
                &bytes,
            )?)
        })
        .await
}
fn event(db: &Store, _: &Ctx<'_, ()>, input: &Input) -> Result<Reply> {
    github_events::record(db, &input.body)?;
    Ok(Reply::ok())
}
