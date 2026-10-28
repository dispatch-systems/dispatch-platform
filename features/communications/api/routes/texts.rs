//! What only a conversation by text has: Retry, a number's contact and block, texting a number or
//! one of today's drivers first, whom the DSP can text, a text's pictures, Today's Routes and Pick
//! driver. A Slack conversation's ID is refused for each.
use super::{DRIVER_ID_MOST, texting_id};
use crate::backend::{model::Reference, phone::numbers::NUMBER_MOST, texts};
use dispatch_core::{
    Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text},
        route::{Dsp, Grant, Member},
        upload::Upload,
    },
};
use std::sync::Arc;

/// The conversation by text the route names.
fn texted(input: &Input) -> Result<String> {
    Reference::parse(input.param("id"))?.texted()
}

/// Sends again a text Dispatch sent in a conversation that failed.
pub(super) async fn retry(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let conversation = texted(&input)?;
    let text = texting_id(input.param("message"))?;
    Reply::of(&texts::reply::retry(&state, &c, access, &conversation, &text).await?)
}
/// Saves a `name` for the number of a conversation by text.
pub(super) fn save_contact(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["name"])?;
    let conversation = texted(input)?;
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
pub(super) fn block(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let conversation = texted(input)?;
    Reply::of(&texts::contacts::block(
        db,
        c.state,
        c.dsp_id(),
        &conversation,
        Some(c.actor()),
    )?)
}
pub(super) fn unblock(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let conversation = texted(input)?;
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
pub(super) fn text_first(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
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
    let number = v::text(&input.body, "number", 3, NUMBER_MOST)?;
    Reply::of(&texts::contacts::open(db, c.state, c.dsp_id(), number)?)
}
/// Whom the DSP can text, or those found by a name or number typed, `q`, and the numbers
/// blocked.
pub(super) fn text_contacts(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let typed = (!typed.is_empty()).then_some(typed);
    Reply::of(&texts::contacts::contacts(db, c.state, c.dsp_id(), typed)?)
}
/// A text's picture or file, as the phone side keeps it.
pub(super) async fn text_file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    let wanted = texting_id(input.param("id"))?;
    texts::reading::file(&state, &c, access, &wanted).await
}
/// Keeps a picture added for a text not sent yet, as the phone side keeps it: its `name` and
/// `type` in the query.
pub(super) async fn add_text_photo(
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
/// Today's Routes: the drivers on a route today, with their work conversations.
pub(super) fn todays_routes(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&texts::todays_routes::read(db, c.state, c.dsp_id())?)
}
/// Files the work phone of a conversation, one nobody held, under one of today's drivers,
/// `driverId`, for today.
pub(super) fn pick_driver(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &["driverId"])?;
    let conversation = texted(input)?;
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
