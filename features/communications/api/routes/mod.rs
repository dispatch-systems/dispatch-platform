//! Communications' endpoints, each registered with its path and access: every one behind
//! `communications.use`, which covers reading the DSP's conversations, Slack's and by text, Today's
//! Routes, and answering them, but the image of someone's initials, which Slack fetches from the
//! admin's address; managing the DSP's dispatch phones, behind `phones.manage` or
//! `connections.manage`; and the phones' own address, by the key each was given (`api::phone`).
//! The server checks the session and the permission before a handler runs; those that call Slack,
//! or wait on bytes, wait outside the database and check again before they write.
//!
//! The handlers, by concern: the list, what changes in it and finding conversations (`inbox`); a
//! conversation of either kind, through the backend's `dispatcher`, and Slack's contacts
//! (`conversations`); what only texts have (`texts`); groups (`groups`); photos, files and
//! pictures (`files`); and the DSP's phones (`phones`). Reading the IDs and pages they name is
//! here, below.
mod conversations;
mod files;
mod groups;
mod inbox;
mod phones;
mod texts;

use crate::backend::{compose, initials, model::Reference, phone::model as texting};
use axum::http::Method;
use dispatch_core::{
    Error, Result,
    db::identifier,
    ensure,
    server::http::{
        input::optional_text,
        route::{Dsp, Route, async_get, async_post, protocol, read, upload, write},
        upload::UPLOAD_LIMIT,
    },
};
use serde_json::Value;

/// Reading the DSP's conversations and answering them.
const USE: Dsp = Dsp("communications.use");
/// The most a message's place may be, in characters, as `before` names it: a text's, thirteen
/// digits, `:` and the ID the phone side gave it; a Slack message's `ts` is shorter.
const PLACE_MOST: usize = 14 + texting::ID_MOST;

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/communications", USE, inbox::overview),
        read("/api/dsp/communications/waiting", USE, inbox::waiting),
        async_get("/api/dsp/communications/updates", USE, inbox::updates),
        read("/api/dsp/communications/conversations", USE, inbox::found),
        async_post(
            "/api/dsp/communications/conversations",
            USE,
            conversations::open,
        ),
        read(
            "/api/dsp/communications/conversations/{id}",
            USE,
            conversations::conversation,
        ),
        read(
            "/api/dsp/communications/conversations/{id}/profile",
            USE,
            conversations::profile,
        ),
        async_post(
            "/api/dsp/communications/conversations/{id}/messages",
            USE,
            conversations::send,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/done",
            USE,
            conversations::done,
        ),
        async_post(
            "/api/dsp/communications/conversations/{id}/typing",
            USE,
            conversations::typing,
        ),
        async_post(
            "/api/dsp/communications/conversations/{id}/messages/{message}/retry",
            USE,
            texts::retry,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/contact",
            USE,
            texts::save_contact,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/block",
            USE,
            texts::block,
        ),
        write(
            "/api/dsp/communications/conversations/{id}/unblock",
            USE,
            texts::unblock,
        ),
        write(
            "/api/dsp/communications/texts/conversations",
            USE,
            texts::text_first,
        ),
        read(
            "/api/dsp/communications/texts/contacts",
            USE,
            texts::text_contacts,
        ),
        async_get(
            "/api/dsp/communications/texts/files/{id}",
            USE,
            texts::text_file,
        ),
        // As large as any upload: the phone side refuses a picture past what a text carries.
        upload(
            "/api/dsp/communications/texts/uploads",
            USE,
            UPLOAD_LIMIT,
            texts::add_text_photo,
        ),
        read("/api/dsp/communications/routes", USE, texts::todays_routes),
        write(
            "/api/dsp/communications/conversations/{id}/driver",
            USE,
            texts::pick_driver,
        ),
        read(
            "/api/dsp/communications/contacts",
            USE,
            conversations::listed,
        ),
        read(
            "/api/dsp/communications/groups/choices",
            USE,
            groups::group_choices,
        ),
        async_post("/api/dsp/communications/groups", USE, groups::create_group),
        read("/api/dsp/communications/groups/{id}", USE, groups::group),
        read(
            "/api/dsp/communications/groups/{id}/people",
            USE,
            groups::group_people,
        ),
        async_post(
            "/api/dsp/communications/groups/{id}/messages",
            USE,
            groups::message_group,
        ),
        write(
            "/api/dsp/communications/groups/{id}/done",
            USE,
            groups::group_done,
        ),
        upload(
            "/api/dsp/communications/uploads",
            USE,
            compose::SLACK.largest,
            files::add_photo,
        ),
        async_get("/api/dsp/communications/files/{id}", USE, files::file),
        read(
            "/api/dsp/communications/workspaces/{team}/logo",
            USE,
            files::logo,
        ),
        protocol(Method::GET, initials::PATH, |_, request| {
            Box::pin(async move { files::picture(request) })
        }),
        read(
            "/api/dsp/communications/phones",
            phones::MANAGE,
            phones::listed,
        ),
        write(
            "/api/dsp/communications/phones/pairing",
            phones::MANAGE,
            phones::new_pairing,
        ),
        read(
            "/api/dsp/communications/phones/pairing/{id}",
            phones::MANAGE,
            phones::pairing_status,
        ),
        write(
            "/api/dsp/communications/phones/{id}/rename",
            phones::MANAGE,
            phones::rename,
        ),
        async_post(
            "/api/dsp/communications/phones/{id}/remove",
            phones::MANAGE,
            phones::remove,
        ),
    ];
    routes.extend(super::phone::routes());
    routes
}

/// One of Communications' IDs, `<prefix>_` and 32 hexadecimal digits.
fn id(value: &str, prefix: &str) -> Result<String> {
    ensure(
        identifier(value, &format!("{prefix}_")),
        "invalid_input",
        400,
    )?;
    Ok(value.to_owned())
}
/// An ID the phone side gave a text or a picture: opaque, as it says one is.
fn texting_id(value: &str) -> Result<String> {
    ensure(texting::identifier(value), "invalid_input", 400)?;
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
/// The workspace `team` a list is of, if it's of one.
fn team(query: &Value) -> Result<Option<String>> {
    let team = optional_text(query, "team", 32)?;
    (!team.is_empty()).then(|| slack_id(team, b"T")).transpose()
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
/// How a photo added for a message in the conversation `reference` is named: by Communications
/// in Slack's, by texting in one by text.
fn photo_id(reference: &Reference) -> fn(&str) -> Result<String> {
    match reference {
        Reference::Slack(_) => file_id,
        Reference::Text(_) => texting_id,
    }
}
