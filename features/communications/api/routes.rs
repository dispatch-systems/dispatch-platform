//! Communications' endpoints, each registered with its path and access: every one behind
//! `communications.use`, which covers reading the DSP's conversations, Slack's and by text, Today's
//! Routes, and answering them, but the image of someone's initials, which Slack fetches from the admin's
//! address. The server checks the session and the permission before a handler runs; those that
//! call Slack or send a text wait outside the database and check again before they write.
use crate::backend::{
    contacts, conversations,
    files::{self, FileStatus, FileStore},
    groups::{self, choices, send as group_send},
    initials, live, profile, reply,
    storage::CommunicationsStore,
    texts::{
        self,
        storage::{ConversationKind, TextStore},
    },
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
use dispatch_phones_contract as texting;
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
        async_post(
            "/api/dsp/communications/conversations/{id}/messages/{message}/retry",
            USE,
            retry,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/contact",
            USE,
            save_contact,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/block",
            USE,
            block,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/unblock",
            USE,
            unblock,
        ),
        write(
            "/api/dsp/communications/texts/conversations",
            USE,
            text_first,
        ),
        read("/api/dsp/communications/texts/contacts", USE, text_contacts),
        async_get("/api/dsp/communications/texts/files/{id}", USE, text_file),
        upload(
            "/api/dsp/communications/texts/uploads",
            USE,
            reply::PHOTO_MOST,
            add_text_photo,
        ),
        read("/api/dsp/communications/routes", USE, todays_routes),
        write(
            "/api/dsp/communications/conversations/{id}/driver",
            USE,
            pick_driver,
        ),
        read("/api/dsp/communications/contacts", USE, listed),
        read("/api/dsp/communications/groups/choices", USE, group_choices),
        async_post("/api/dsp/communications/groups", USE, create_group),
        read("/api/dsp/communications/groups/{id}", USE, group),
        read(
            "/api/dsp/communications/groups/{id}/people",
            USE,
            group_people,
        ),
        async_post(
            "/api/dsp/communications/groups/{id}/messages",
            USE,
            message_group,
        ),
        write("/api/dsp/communications/groups/{id}/done", USE, group_done),
        upload(
            "/api/dsp/communications/uploads",
            USE,
            reply::PHOTO_MOST,
            add_photo,
        ),
        async_get("/api/dsp/communications/files/{id}", USE, file),
        read("/api/dsp/communications/workspaces/{team}/logo", USE, logo),
        protocol(Method::GET, initials::PATH, |_, request| {
            Box::pin(async move { picture(request) })
        }),
    ]
}

/// The most a driver's ID may be, in characters, as Routes reads them.
const DRIVER_ID_MOST: usize = 128;

/// One of Communications' IDs, `<prefix>_` and 32 hexadecimal digits.
fn id(value: &str, prefix: &str) -> Result<String> {
    let valid = value
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('_'))
        .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()));
    ensure(valid, "invalid_input", 400)?;
    Ok(value.to_owned())
}
/// An ID texting gave a text or a picture, as its contract says one is: opaque.
fn texting_id(value: &str) -> Result<String> {
    ensure(texting::identifier(value), "invalid_input", 400)?;
    Ok(value.to_owned())
}
/// A conversation's ID, a Slack one's (`cv_…`) or one by text (`tc_…`): the ID, and whether it's
/// by text.
fn conversation_id(value: &str) -> Result<(String, bool)> {
    match id(value, "cv") {
        Ok(slack) => Ok((slack, false)),
        Err(_) => Ok((id(value, "tc")?, true)),
    }
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

/// The conversations waiting, and the latest answered of the workspace `team` or every one.
fn overview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["team"])?;
    let team = team(&input.query)?;
    Reply::of(&conversations::overview(
        db,
        c.state,
        c.dsp_id(),
        c.actor(),
        team.as_deref(),
    )?)
}
fn waiting(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&conversations::waiting(db, c.state, c.dsp_id())?)
}
/// The workspace `team` a list is of, if it's of one.
fn team(query: &Value) -> Result<Option<String>> {
    let team = optional_text(query, "team", 32)?;
    (!team.is_empty()).then(|| slack_id(team, b"T")).transpose()
}
/// Conversations found by a name typed, `q`, or the answered ones before `before`, in the
/// workspace `team` or every one, or the numbers' (`kind=number`); or work conversations found by
/// a name or digits typed (`kind=work`).
fn found(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q", "team", "before", "kind"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let team = team(&input.query)?;
    let texted = match optional_text(&input.query, "kind", 10)? {
        "" => None,
        "number" if team.is_none() => Some(ConversationKind::Number),
        "work" if team.is_none() && !typed.is_empty() => Some(ConversationKind::Work),
        _ => return Err(Error::new("invalid_input", 400)),
    };
    if let Some(kind) = texted {
        if !typed.is_empty() {
            return Reply::of(&texts::reading::search(
                db,
                c.state,
                c.dsp_id(),
                kind,
                typed,
            )?);
        }
        let (at, id) = before(&input.query)?.ok_or_else(|| Error::new("invalid_input", 400))?;
        return Reply::of(&texts::reading::older(db, c.state, c.dsp_id(), (at, &id))?);
    }
    if !typed.is_empty() {
        return Reply::of(&conversations::search(
            db,
            c.state,
            c.dsp_id(),
            team.as_deref(),
            typed,
        )?);
    }
    let (at, id) = before(&input.query)?.ok_or_else(|| Error::new("invalid_input", 400))?;
    Reply::of(&conversations::older(
        db,
        c.state,
        c.dsp_id(),
        team.as_deref(),
        (at, &id),
    )?)
}
fn conversation(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["before"])?;
    let (conversation, texted) = conversation_id(input.param("id"))?;
    if texted {
        let before = optional_text(&input.query, "before", 80)?;
        let before = (!before.is_empty())
            .then(|| texts::reading::before(before))
            .transpose()?;
        return Reply::of(&texts::reading::opened(
            db,
            c.state,
            c.dsp_id(),
            &conversation,
            before.as_ref().map(|(at, id)| (*at, id.as_str())),
        )?);
    }
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
/// What the profile beside a conversation shows of whom it's with, a member of a workspace or a
/// driver: behind `communications.use` alone, as it reads what other features know of them for
/// the page.
fn profile(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let (conversation, texted) = conversation_id(input.param("id"))?;
    Reply::of(&profile::of(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        texted,
    )?)
}
/// Today's Routes: the drivers on a route today, with their work conversations.
fn todays_routes(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&texts::todays_routes::read(db, c.state, c.dsp_id())?)
}
/// Files the work phone of a conversation, one nobody held, under one of today's drivers,
/// `driverId`, for today.
fn pick_driver(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["driverId"])?;
    let conversation = id(input.param("id"), "tc")?;
    let driver = v::text(&input.body, "driverId", 1, DRIVER_ID_MOST)?;
    Reply::of(&texts::picking::pick(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        driver,
        c.actor(),
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
    let (conversation, texted) = conversation_id(input.param("id"))?;
    if texted {
        let found = db.communications_text_conversation(c.dsp_id(), &conversation)?;
        ensure(
            found.is_some(),
            "communications_conversation_not_found",
            404,
        )?;
        if db.mark_communications_text_done(c.dsp_id(), &conversation, c.actor())? {
            live::changed(c.state, c.dsp_id());
        }
        return Reply::of(&texts::reading::one(
            db,
            c.state,
            c.dsp_id(),
            &conversation,
        )?);
    }
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

/// Whom a group message can go to, and the groups to choose from, today being the DSP's own day.
fn group_choices(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&choices::choices(db, c.state, c.dsp_id(), &c.dsp.timezone)?)
}
/// A group and its latest messages and answers, or those before `before`, `<time>:<id>`.
fn group(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["before"])?;
    let group = id(input.param("id"), "grp")?;
    let before = before(&input.query)?;
    Reply::of(&groups::opened(
        db,
        c.state,
        c.dsp_id(),
        &group,
        before.as_ref().map(|(at, id)| (*at, id.as_str())),
    )?)
}
/// Who a group went to, and where it stands with each.
fn group_people(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let group = id(input.param("id"), "grp")?;
    Reply::of(&groups::people(db, c.state, c.dsp_id(), &group)?)
}
/// Marks done every conversation listed under a group that waits.
fn group_done(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let group = id(input.param("id"), "grp")?;
    Reply::of(&groups::done(db, c.state, c.dsp_id(), &group, c.actor())?)
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
/// Sends a message in a conversation: its `text`, and the `photos` added for it; by text, the
/// text alone.
async fn send(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["text", "photos"])?;
    let (conversation, texted) = conversation_id(input.param("id"))?;
    let text = optional_text(&input.body, "text", reply::TEXT_MOST * 2)?;
    let message = if texted {
        let photos = photos(&input.body, texting_id)?;
        texts::reply::send(&state, &c, access, &conversation, text, &photos).await?
    } else {
        let photos = photos(&input.body, file_id)?;
        reply::send(&state, &c, access, &conversation, text, &photos).await?
    };
    Reply::of_status(&message, 201)
}
/// Sends again a text Dispatch sent in a conversation that failed.
async fn retry(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let conversation = id(input.param("id"), "tc")?;
    let text = texting_id(input.param("message"))?;
    Reply::of(&texts::reply::retry(&state, &c, access, &conversation, &text).await?)
}
/// Saves a `name` for the number of a conversation by text.
fn save_contact(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["name"])?;
    let conversation = id(input.param("id"), "tc")?;
    let name = v::name(&input.body, "name", texts::contacts::NAME_MOST)?;
    Reply::of(&texts::contacts::save_contact(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        &name,
        c.actor(),
    )?)
}
/// Blocks the number of a conversation by text.
fn block(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let conversation = id(input.param("id"), "tc")?;
    Reply::of(&texts::contacts::block(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        Some(c.actor()),
    )?)
}
fn unblock(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let conversation = id(input.param("id"), "tc")?;
    Reply::of(&texts::contacts::block(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        None,
    )?)
}
/// Opens the conversation with a `number`, or one of today's drivers, `driverId`, to text them
/// first.
fn text_first(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["number", "driverId"])?;
    if input.body.get("driverId").is_some() {
        ensure(input.body.get("number").is_none(), "invalid_input", 400)?;
        let driver = v::text(&input.body, "driverId", 1, DRIVER_ID_MOST)?;
        return Reply::of(&texts::contacts::open_driver(
            db,
            c.state,
            c.dsp_id(),
            driver,
        )?);
    }
    let number = v::text(&input.body, "number", 3, 32)?;
    Reply::of(&texts::contacts::open(db, c.state, c.dsp_id(), number)?)
}
/// Whom the DSP can text, or those found by a name or number typed, `q`, and the numbers
/// blocked.
fn text_contacts(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let typed = (!typed.is_empty()).then_some(typed);
    Reply::of(&texts::contacts::contacts(db, c.state, c.dsp_id(), typed)?)
}
/// A text's picture or file, as what offers the DSP texting keeps it.
async fn text_file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let wanted = texting_id(input.param("id"))?;
    texts::reading::file(&state, &c, access, &wanted).await
}
/// The `photos` added for a message: their IDs, each as `checked` checks one.
fn photos(body: &Value, checked: fn(&str) -> Result<String>) -> Result<Vec<String>> {
    match body.get("photos") {
        None => Ok(Vec::new()),
        Some(photos) => photos
            .as_array()
            .ok_or_else(|| Error::new("invalid_input", 400))?
            .iter()
            .map(|photo| checked(photo.as_str().unwrap_or_default()))
            .collect(),
    }
}
/// One of the photos Communications keeps for a message, by its ID.
fn file_id(value: &str) -> Result<String> {
    id(value, "file")
}
/// Messages a group: its `name`, its `people`, each `teamId` and `userId`, and the `text` and the
/// `photos` added for it.
async fn create_group(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["name", "people", "text", "photos"])?;
    let name = optional_text(&input.body, "name", group_send::NAME_MOST * 4)?;
    let people = input
        .body
        .get("people")
        .and_then(Value::as_array)
        .filter(|people| people.len() <= group_send::PEOPLE_MOST)
        .ok_or_else(|| Error::new("invalid_input", 400))?
        .iter()
        .map(|member| {
            v::fields(member, &["teamId", "userId"])?;
            Ok((
                slack_id(v::text(member, "teamId", 2, 32)?, b"T")?,
                slack_id(v::text(member, "userId", 2, 32)?, b"UW")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let text = optional_text(&input.body, "text", reply::TEXT_MOST * 2)?;
    let photos = photos(&input.body, file_id)?;
    let group = group_send::create(&state, &c, access, name, &people, text, &photos).await?;
    group_send::send_soon(&state, &c.dsp.id);
    Reply::of_status(&group, 201)
}
/// Sends a group another message, to the same people: its `text` and the `photos` added for it.
async fn message_group(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["text", "photos"])?;
    let group = id(input.param("id"), "grp")?;
    let text = optional_text(&input.body, "text", reply::TEXT_MOST * 2)?;
    let photos = photos(&input.body, file_id)?;
    let entry = group_send::again(&state, &c, access, &group, text, &photos).await?;
    group_send::send_soon(&state, &c.dsp.id);
    Reply::of_status(&entry, 201)
}
/// Notes that the member is writing in a conversation, for the next few seconds.
async fn typing(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let (conversation, texted) = conversation_id(input.param("id"))?;
    let of = conversation.clone();
    let c = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            let found = if texted {
                db.communications_text_conversation(&c.dsp.id, &of)?
                    .is_some()
            } else {
                db.communications_conversation(&c.dsp.id, &of)?.is_some()
            };
            ensure(found, "communications_conversation_not_found", 404)?;
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
/// Keeps a picture added for a text not sent yet, through what offers the DSP texting: its
/// `name` and `type` in the query.
async fn add_text_photo(
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
    let added = texts::reply::upload(&state, &c, access, &name, kind, photo).await?;
    Reply::of_status(&added, 201)
}
/// A workspace's logo, as Slack keeps a copy of it. Its address carries its version, `v`, so the
/// browser keeps it until the logo changes.
fn logo(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["v"])?;
    let team = slack_id(input.param("team"), b"T")?;
    let (kind, picture) =
        conversations::logo(db, c.dsp_id(), &team)?.ok_or_else(|| Error::new("not_found", 404))?;
    Ok(Reply::picture(kind, picture.into()))
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
