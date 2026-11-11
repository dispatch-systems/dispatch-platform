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
//! pictures (`files`); the DSP's quick replies (`replies`); telling the member of new messages
//! with Dispatch closed (`push`); and the DSP's phones (`phones`). Reading the IDs and pages they
//! name is here, below. Its writes of conversations change only Communications' own data (`changes`), so
//! they leave the DSP's other cached reads, as who is who, as they were.
mod conversations;
mod files;
mod gifs;
mod groups;
mod inbox;
mod phones;
mod push;
mod replies;
mod texts;

use crate::{
    api::types::MessageGif,
    backend::{
        compose, gifs as chosen, initials, model::Reference, phone::model as texting,
        storage::DOMAIN,
    },
};
use axum::http::Method;
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::identifier,
    ensure,
    server::http::{
        input::optional_text,
        route::{Dsp, Public, Route, async_get, async_post, protocol, read, upload, write},
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
        read("/api/dsp/dispatch-phone", USE, inbox::overview).also_at("/api/dsp/communications"),
        read("/api/dsp/dispatch-phone/waiting", USE, inbox::waiting)
            .also_at("/api/dsp/communications/waiting"),
        async_get("/api/dsp/dispatch-phone/updates", USE, inbox::updates)
            .also_at("/api/dsp/communications/updates"),
        read("/api/dsp/dispatch-phone/conversations", USE, inbox::found)
            .also_at("/api/dsp/communications/conversations"),
        async_post(
            "/api/dsp/dispatch-phone/conversations",
            USE,
            conversations::open,
        )
        .also_at("/api/dsp/communications/conversations"),
        read(
            "/api/dsp/dispatch-phone/conversations/{id}",
            USE,
            conversations::conversation,
        )
        .also_at("/api/dsp/communications/conversations/{id}"),
        read(
            "/api/dsp/dispatch-phone/conversations/{id}/window",
            USE,
            conversations::window,
        )
        .also_at("/api/dsp/communications/conversations/{id}/window"),
        read(
            "/api/dsp/dispatch-phone/conversations/{id}/profile",
            USE,
            conversations::profile,
        )
        .also_at("/api/dsp/communications/conversations/{id}/profile"),
        async_post(
            "/api/dsp/dispatch-phone/conversations/{id}/messages",
            USE,
            conversations::send,
        )
        .also_at("/api/dsp/communications/conversations/{id}/messages"),
        read("/api/dsp/dispatch-phone/gifs", USE, gifs::search)
            .also_at("/api/dsp/communications/gifs"),
        read(
            "/api/dispatch-phone/gifs/fixture/gifs/{list}",
            Public,
            gifs::fixture_results,
        )
        .also_at("/api/communications/gifs/fixture/gifs/{list}"),
        async_post(
            "/api/dispatch-phone/gifs/fixture/gifs/share/{slug}",
            Public,
            gifs::fixture_shared,
        )
        .also_at("/api/communications/gifs/fixture/gifs/share/{slug}"),
        read(
            "/api/dispatch-phone/gifs/fixture/media/{name}",
            Public,
            gifs::fixture_media,
        )
        .also_at("/api/communications/gifs/fixture/media/{name}"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/done",
            USE,
            conversations::done,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/conversations/{id}/done"),
        async_post(
            "/api/dsp/dispatch-phone/conversations/{id}/typing",
            USE,
            conversations::typing,
        )
        .also_at("/api/dsp/communications/conversations/{id}/typing"),
        async_post(
            "/api/dsp/dispatch-phone/conversations/{id}/messages/{message}/retry",
            USE,
            texts::retry,
        )
        .also_at("/api/dsp/communications/conversations/{id}/messages/{message}/retry"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/contact",
            USE,
            texts::save_contact,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/conversations/{id}/contact"),
        // Changes who the number is, which every feature showing people reads: not only its own.
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/person",
            USE,
            texts::add_to_person,
        )
        .also_at("/api/dsp/communications/conversations/{id}/person"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/person/remove",
            USE,
            texts::remove_from_person,
        )
        .also_at("/api/dsp/communications/conversations/{id}/person/remove"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/block",
            USE,
            texts::block,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/conversations/{id}/block"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/unblock",
            USE,
            texts::unblock,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/conversations/{id}/unblock"),
        write(
            "/api/dsp/dispatch-phone/texts/conversations",
            USE,
            texts::text_first,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/texts/conversations"),
        read(
            "/api/dsp/dispatch-phone/texts/contacts",
            USE,
            texts::text_contacts,
        )
        .also_at("/api/dsp/communications/texts/contacts"),
        read("/api/dsp/dispatch-phone/texts/people", USE, texts::addable)
            .also_at("/api/dsp/communications/texts/people"),
        async_get(
            "/api/dsp/dispatch-phone/texts/files/{id}",
            USE,
            texts::text_file,
        )
        .also_at("/api/dsp/communications/texts/files/{id}"),
        async_get(
            "/api/dsp/dispatch-phone/texts/files/{id}/preview",
            USE,
            texts::text_file_preview,
        )
        .also_at("/api/dsp/communications/texts/files/{id}/preview"),
        async_post(
            "/api/dsp/dispatch-phone/texts/files/{id}/fetch",
            USE,
            texts::fetch_text_file,
        )
        .also_at("/api/dsp/communications/texts/files/{id}/fetch"),
        // As large as any upload: the phone side refuses a picture past what a text carries.
        upload(
            "/api/dsp/dispatch-phone/texts/uploads",
            USE,
            UPLOAD_LIMIT,
            texts::add_text_photo,
        )
        .also_at("/api/dsp/communications/texts/uploads"),
        read("/api/dsp/dispatch-phone/routes", USE, texts::todays_routes)
            .also_at("/api/dsp/communications/routes"),
        read(
            "/api/dsp/dispatch-phone/quick-replies",
            USE,
            replies::quick_replies,
        )
        .also_at("/api/dsp/communications/quick-replies"),
        write(
            "/api/dsp/dispatch-phone/quick-replies",
            USE,
            replies::save_quick_replies,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/quick-replies"),
        write(
            "/api/dsp/dispatch-phone/conversations/{id}/driver",
            USE,
            texts::pick_driver,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/conversations/{id}/driver"),
        read(
            "/api/dsp/dispatch-phone/contacts",
            USE,
            conversations::listed,
        )
        .also_at("/api/dsp/communications/contacts"),
        read(
            "/api/dsp/dispatch-phone/groups/choices",
            USE,
            groups::group_choices,
        )
        .also_at("/api/dsp/communications/groups/choices"),
        async_post("/api/dsp/dispatch-phone/groups", USE, groups::create_group)
            .also_at("/api/dsp/communications/groups"),
        read("/api/dsp/dispatch-phone/groups/{id}", USE, groups::group)
            .also_at("/api/dsp/communications/groups/{id}"),
        read(
            "/api/dsp/dispatch-phone/groups/{id}/people",
            USE,
            groups::group_people,
        )
        .also_at("/api/dsp/communications/groups/{id}/people"),
        async_post(
            "/api/dsp/dispatch-phone/groups/{id}/messages",
            USE,
            groups::message_group,
        )
        .also_at("/api/dsp/communications/groups/{id}/messages"),
        write(
            "/api/dsp/dispatch-phone/groups/{id}/done",
            USE,
            groups::group_done,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/groups/{id}/done"),
        upload(
            "/api/dsp/dispatch-phone/uploads",
            USE,
            compose::SLACK.largest,
            files::add_photo,
        )
        .also_at("/api/dsp/communications/uploads"),
        async_get("/api/dsp/dispatch-phone/files/{id}", USE, files::file)
            .also_at("/api/dsp/communications/files/{id}"),
        async_get(
            "/api/dsp/dispatch-phone/files/{id}/preview",
            USE,
            files::file_preview,
        )
        .also_at("/api/dsp/communications/files/{id}/preview"),
        read(
            "/api/dsp/dispatch-phone/workspaces/{team}/logo",
            USE,
            files::logo,
        )
        .also_at("/api/dsp/communications/workspaces/{team}/logo"),
        read("/api/dsp/dispatch-phone/push", USE, push::key)
            .also_at("/api/dsp/communications/push"),
        async_post("/api/dsp/dispatch-phone/push", USE, push::subscribe)
            .also_at("/api/dsp/communications/push"),
        write(
            "/api/dsp/dispatch-phone/push/remove",
            USE,
            push::unsubscribe,
        )
        .changes(DOMAIN)
        .also_at("/api/dsp/communications/push/remove"),
        protocol(Method::GET, initials::PATH, |_, request| {
            Box::pin(async move { files::picture(request) })
        })
        .also_at("/communications/initials/1/{name}"),
        read(
            "/api/dsp/dispatch-phone/phones",
            phones::MANAGE,
            phones::listed,
        )
        .also_at("/api/dsp/communications/phones"),
        write(
            "/api/dsp/dispatch-phone/phones/pairing",
            phones::MANAGE,
            phones::new_pairing,
        )
        .also_at("/api/dsp/communications/phones/pairing"),
        read(
            "/api/dsp/dispatch-phone/phones/pairing/{id}",
            phones::MANAGE,
            phones::pairing_status,
        )
        .also_at("/api/dsp/communications/phones/pairing/{id}"),
        write(
            "/api/dsp/dispatch-phone/phones/{id}/rename",
            phones::MANAGE,
            phones::rename,
        )
        .also_at("/api/dsp/communications/phones/{id}/rename"),
        async_post(
            "/api/dsp/dispatch-phone/phones/{id}/remove",
            phones::MANAGE,
            phones::remove,
        )
        .also_at("/api/dsp/communications/phones/{id}/remove"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
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
    place(query, "before")
}
/// A place among messages listed by when each was written and their ID, `<time>:<id>`, as `key`
/// names it: none without one.
fn place(value: &Value, key: &str) -> Result<Option<(i64, String)>> {
    let before = optional_text(value, key, 100)?;
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
/// The `gif` chosen for a message, as KLIPY gave the page it, if any: only while the DSP has GIFs.
fn gif(body: &Value, c: &Context, fixture: bool) -> Result<Option<MessageGif>> {
    let Some(gif) = body.get("gif").filter(|gif| !gif.is_null()) else {
        return Ok(None);
    };
    ensure(c.has(chosen::PART), "not_found", 404)?;
    let gif = serde_json::from_value(gif.clone()).map_err(|_| Error::new("invalid_input", 400))?;
    chosen::checked(gif, fixture).map(Some)
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
