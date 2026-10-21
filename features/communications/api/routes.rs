//! Communications' endpoints, each registered with its path and access: every one behind
//! `communications.use`, which covers reading the DSP's conversations and answering them, but
//! the image of someone's initials, which Slack fetches from the admin's address. The server
//! checks the session and the permission before a handler runs; those that call Slack wait
//! outside the database and check again before they write.
use crate::backend::{
    contacts, conversations,
    files::{self, FileStatus, FileStore},
    initials, live, profile, reply,
    storage::CommunicationsStore,
};
use axum::{
    body::Body,
    extract::Request,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text},
        route::{Dsp, Grant, Member, Route, async_get, async_post, protocol, read, upload, write},
        upload::Upload,
    },
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

/// Reading the DSP's conversations and answering them.
const USE: Dsp = Dsp("communications.use");
/// How long a page waiting on changes waits before it's answered with none.
const WAIT: Duration = Duration::from_secs(20);

pub fn routes() -> Vec<Route> {
    vec![
        read("/api/dsp/communications", USE, overview),
        read("/api/dsp/communications/waiting", USE, waiting),
        async_get("/api/dsp/communications/updates", USE, updates),
        read("/api/dsp/communications/conversations", USE, found),
        async_post("/api/dsp/communications/conversations", USE, open),
        read(
            "/api/dsp/communications/conversations/{id}",
            USE,
            conversation,
        ),
        read(
            "/api/dsp/communications/conversations/{id}/profile",
            USE,
            profile,
        ),
        async_post(
            "/api/dsp/communications/conversations/{id}/messages",
            USE,
            send,
        ),
        write("/api/dsp/communications/conversations/{id}/done", USE, done),
        async_post(
            "/api/dsp/communications/conversations/{id}/typing",
            USE,
            typing,
        ),
        read("/api/dsp/communications/contacts", USE, listed),
        upload(
            "/api/dsp/communications/uploads",
            USE,
            reply::PHOTO_MOST,
            add_photo,
        ),
        async_get("/api/dsp/communications/files/{id}", USE, file),
        protocol(Method::GET, initials::PATH, |_, request| {
            Box::pin(async move { picture(request) })
        }),
    ]
}

/// One of Communications' IDs, `<prefix>_` and 32 hexadecimal digits.
fn id(value: &str, prefix: &str) -> Result<String> {
    let valid = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
    ensure(valid, "invalid_input", 400)?;
    Ok(value.to_owned())
}
/// A Slack ID beginning with one of `first`: a workspace's `T…`, a member's `U…` or `W…`.
fn slack_id(value: &str, first: &[u8]) -> Result<String> {
    let valid = (2..=32).contains(&value.len())
        && first.contains(&value.as_bytes()[0])
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    ensure(valid, "invalid_input", 400)?;
    Ok(value.to_owned())
}
/// Where a page of the latest-first left off, `before=<time>:<id>`: none for the first page.
fn before(query: &Value) -> Result<Option<(i64, String)>> {
    let before = optional_text(query, "before", 100)?;
    if before.is_empty() {
        return Ok(None);
    }
    let (at, id) = before
        .split_once(':')
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let at = at
        .parse::<i64>()
        .map_err(|_| Error::new("invalid_input", 400))?;
    Ok(Some((at, id.to_owned())))
}

fn overview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&conversations::overview(
        db,
        c.state,
        c.dsp_id(),
        c.actor(),
    )?)
}
fn waiting(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&conversations::waiting(db, c.dsp_id())?)
}
/// The workspace `team` a list is of, if it's of one.
fn team(query: &Value) -> Result<Option<String>> {
    let team = optional_text(query, "team", 32)?;
    (!team.is_empty())
        .then(|| slack_id(team, b"T"))
        .transpose()
}
/// Conversations found by a name typed, `q`, in the workspace `team` or every one, or the
/// answered ones before `before`.
fn found(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q", "team", "before"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let team = team(&input.query)?;
    if !typed.is_empty() {
        return Reply::of(&conversations::search(
            db,
            c.state,
            c.dsp_id(),
            team.as_deref(),
            typed,
        )?);
    }
    ensure(team.is_none(), "invalid_input", 400)?;
    let (at, id) = before(&input.query)?.ok_or_else(|| Error::new("invalid_input", 400))?;
    Reply::of(&conversations::older(db, c.state, c.dsp_id(), (at, &id))?)
}
fn conversation(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["before"])?;
    let conversation = id(input.param("id"), "cv")?;
    let before = optional_text(&input.query, "before", 32)?;
    let valid = before.split_once('.').is_some_and(|(seconds, micros)| {
        [seconds, micros]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    });
    ensure(before.is_empty() || valid, "invalid_input", 400)?;
    let before = (!before.is_empty()).then_some(before);
    Reply::of(&conversations::opened(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        before,
    )?)
}
/// What the profile beside a conversation shows of the member it's with: behind
/// `communications.use` alone, as it reads what other features know of them for the page.
fn profile(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let conversation = id(input.param("id"), "cv")?;
    Reply::of(&profile::of(
        db,
        c.state,
        c.dsp_id(),
        &c.dsp.timezone,
        &conversation,
    )?)
}
/// Whom the DSP can message, in the workspace `team` or every one, or those found by a name
/// typed, `q`.
fn listed(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q", "team"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let typed = (!typed.is_empty()).then_some(typed);
    let team = team(&input.query)?;
    Reply::of(&contacts::contacts(
        db,
        c.state,
        c.dsp_id(),
        team.as_deref(),
        typed,
    )?)
}
fn done(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let conversation = id(input.param("id"), "cv")?;
    let found = db.communications_conversation(c.dsp_id(), &conversation)?;
    ensure(
        found.is_some(),
        "communications_conversation_not_found",
        404,
    )?;
    if db.mark_communications_done(c.dsp_id(), &conversation, c.actor())? {
        live::changed(c.state, c.dsp_id());
    }
    Reply::of(&conversations::one(db, c.state, c.dsp_id(), &conversation)?)
}

/// Opens a direct message with a member of one of the DSP's workspaces: `teamId` and `userId`.
async fn open(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["teamId", "userId"])?;
    let team = slack_id(v::text(&input.body, "teamId", 2, 32)?, b"T")?;
    let user = slack_id(v::text(&input.body, "userId", 2, 32)?, b"UW")?;
    Reply::of(&reply::open(&state, &c, access, &team, &user).await?)
}
/// Sends a message in a conversation: its `text`, and the `photos` added for it.
async fn send(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["text", "photos"])?;
    let conversation = id(input.param("id"), "cv")?;
    let text = optional_text(&input.body, "text", reply::TEXT_MOST * 2)?;
    let photos = match input.body.get("photos") {
        None => Vec::new(),
        Some(photos) => photos
            .as_array()
            .ok_or_else(|| Error::new("invalid_input", 400))?
            .iter()
            .map(|photo| id(photo.as_str().unwrap_or_default(), "file"))
            .collect::<Result<_>>()?,
    };
    let message = reply::send(&state, &c, access, &conversation, text, &photos).await?;
    Reply::of_status(&message, 201)
}
/// Notes that the member is writing in a conversation, for the next few seconds.
async fn typing(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let conversation = id(input.param("id"), "cv")?;
    let of = conversation.clone();
    let c = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            let found = db.communications_conversation(&c.dsp.id, &of)?;
            ensure(
                found.is_some(),
                "communications_conversation_not_found",
                404,
            )?;
            Ok(c)
        })
        .await?;
    v::fields(&input.body, &[])?;
    live::writing(&state, &c.dsp.id, &conversation, &c.auth.user);
    Ok(Reply::ok())
}
/// Keeps a photo added for a message not sent yet: its `name` and `type` in the query.
async fn add_photo(state: Arc<State>, input: Input, access: Dsp, photo: Upload) -> Result<Reply> {
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
async fn file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
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
        .filter(|file| file.message.is_some() || file.uploaded_by.as_ref() == Some(&c.auth.user.id))
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

/// What a page waiting on changes is answered with: what changed since the change `after`, and
/// who is writing, once either differs from what it has (`typing`, the version of that), or
/// twenty seconds pass.
async fn updates(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    v::fields(&input.query, &["after", "typing"])?;
    let number = |key: &str| -> Result<i64> {
        let text = optional_text(&input.query, key, 20)?;
        if text.is_empty() {
            return Ok(-1);
        }
        text.parse().map_err(|_| Error::new("invalid_input", 400))
    };
    let (after, seen) = (number("after")?, number("typing")?);
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    let _slot = state
        .topics
        .slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| Error::new("platform_busy", 503))?;
    // Subscribed before reading, so a change between the read and the wait still wakes it.
    let mut listener = state.topics.subscribe(live::TOPIC, &c.dsp.id);
    let until = tokio::time::Instant::now() + WAIT;
    loop {
        let (asked, shared) = (input.clone(), state.clone());
        let (answer, next) = state
            .read(move |db| {
                let c = access.authorize(db, &asked)?;
                conversations::updates(db, &shared, &c.dsp.id, after, &c.auth.user.id)
            })
            .await?;
        let now = tokio::time::Instant::now();
        if answer.revision != after || answer.typing.version as i64 != seen || now >= until {
            return Reply::of(&answer);
        }
        // Woken by a change, or by the next of those writing stopping, or at the end.
        let stops = next
            .map(|at| now + Duration::from_millis((at - dispatch_core::db::now()).max(0) as u64));
        let wake = stops.map_or(until, |stops| stops.min(until));
        let _ = tokio::time::timeout_at(wake, listener.changed()).await;
    }
}

/// The image of someone's initials, which Slack shows their messages under: public, as Slack
/// fetches it from wherever its link points, and showing two letters and nothing else.
fn picture(request: Request) -> Response {
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
