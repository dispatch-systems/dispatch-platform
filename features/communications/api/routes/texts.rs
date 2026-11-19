//! What only a conversation by text has: Retry, a number's contact, person and block, texting a number or
//! one of today's drivers first, whom the DSP can text, a text's pictures and Today's Routes. A
//! Slack conversation's ID is refused for each. Retry, a number's contact and block,
//! texting a number first, whom the DSP can text and a text's pictures are each of one line
//! (`*_of`): Dispatch Phone's own here, another feature's from its own routes (`api::lines`); the
//! rest are Dispatch Phone's own.
use super::{served, texting_id};
use crate::backend::{
    access::Access,
    model::Reference,
    phone::{numbers::NUMBER_MOST, storage::Line},
    texts,
};
use dispatch_core::{
    Result, State,
    db::Store,
    ensure,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional, optional_text},
        route::{Dsp, Member},
        upload::Upload,
    },
};
use dispatch_routes_contract::MOST_ID_LENGTH;
use std::sync::Arc;

/// The conversation by text the route names.
fn texted(input: &Input) -> Result<String> {
    Reference::parse(input.param("id"))?.texted()
}

/// Sends again a text Dispatch sent in a conversation that failed.
pub(super) async fn retry(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    retry_of(state, input, access, None).await
}
/// Sends again a text Dispatch sent in a conversation of `line` that failed, from that line's
/// phones.
pub async fn retry_of(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    line: Option<Line<'static>>,
) -> Result<Reply> {
    let (asked, access) = (input.clone(), Access::of(access, line));
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &[])?;
    let conversation = texted(&input)?;
    let text = texting_id(input.param("message"))?;
    Reply::of(&texts::reply::retry(&state, &c, access, &conversation, &text).await?)
}
/// Saves a `name` for the number of a conversation by text.
pub(super) fn save_contact(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    save_contact_of(db, c, input, None)
}
/// Saves a `name` for the number of a conversation of `line`, that line's alone.
pub fn save_contact_of(db: &Store, c: &Member, input: &Input, line: Option<Line>) -> Result<Reply> {
    let line = line.map(|line| line.id);
    served(c, line)?;
    v::fields(&input.body, &["name"])?;
    let conversation = texted(input)?;
    let name = v::name(&input.body, "name", texts::contacts::NAME_MOST)?;
    Reply::of(&texts::contacts::save_contact(
        db,
        c.state,
        c.dsp_id(),
        line,
        &conversation,
        &name,
        c.actor(),
    )?)
}
/// Adds the number of a conversation by text to the person `code`'s.
pub(super) fn add_to_person(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    person(db, c, input, true)
}
/// Takes the number of a conversation by text off the person `code`, whom it was added to.
pub(super) fn remove_from_person(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    person(db, c, input, false)
}
fn person(db: &Store, c: &Member, input: &Input, added: bool) -> Result<Reply> {
    v::fields(&input.body, &["code"])?;
    let conversation = texted(input)?;
    let code = v::text(&input.body, "code", 6, 6)?;
    Reply::of(&texts::contacts::add_to_person(
        db,
        c.state,
        c,
        &conversation,
        code,
        added,
    )?)
}
/// Everyone Driver Match knows who hasn't left, whom a number can be added to.
pub(super) fn addable(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&texts::contacts::addable(db, c.state, c.dsp_id())?)
}
/// Blocks the number of a conversation by text.
pub(super) fn block(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    block_of(db, c, input, None)
}
pub(super) fn unblock(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    unblock_of(db, c, input, None)
}
/// Blocks the number of a conversation of `line`, on that line alone.
pub fn block_of(db: &Store, c: &Member, input: &Input, line: Option<Line>) -> Result<Reply> {
    blocking(db, c, input, line, true)
}
/// Unblocks the number of a conversation of `line`, on that line alone.
pub fn unblock_of(db: &Store, c: &Member, input: &Input, line: Option<Line>) -> Result<Reply> {
    blocking(db, c, input, line, false)
}
fn blocking(
    db: &Store,
    c: &Member,
    input: &Input,
    line: Option<Line>,
    blocked: bool,
) -> Result<Reply> {
    let line = line.map(|line| line.id);
    served(c, line)?;
    v::fields(&input.body, &[])?;
    let conversation = texted(input)?;
    Reply::of(&texts::contacts::block(
        db,
        c.state,
        c.dsp_id(),
        line,
        &conversation,
        c.actor(),
        blocked,
    )?)
}
/// Opens the conversation with a `number`, or one of today's drivers, `driverId`, to text them
/// first.
pub(super) fn text_first(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    text_first_of(db, c, input, None)
}
/// Opens the conversation of `line` with a `number` to text it first; or on Dispatch Phone's own,
/// one of today's drivers', `driverId`.
pub fn text_first_of(db: &Store, c: &Member, input: &Input, line: Option<Line>) -> Result<Reply> {
    served(c, line.map(|line| line.id))?;
    // Today's drivers are Dispatch Phone's own.
    let fields: &[&str] = match line {
        None => &["number", "driverId"],
        Some(_) => &["number"],
    };
    v::fields(&input.body, fields)?;
    if input.body.get("driverId").is_some() {
        ensure(input.body.get("number").is_none(), "invalid_input", 400)?;
        let driver = v::text(&input.body, "driverId", 1, MOST_ID_LENGTH)?;
        return Reply::of(&texts::contacts::open_driver(
            db,
            c.state,
            c.dsp_id(),
            driver,
        )?);
    }
    let number = v::text(&input.body, "number", 3, NUMBER_MOST)?;
    Reply::of(&texts::contacts::open(
        db,
        c.state,
        c.dsp_id(),
        line,
        number,
    )?)
}
/// Whom the DSP can text, or those found by a name or number typed, `q`, and the numbers
/// blocked.
pub(super) fn text_contacts(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    text_contacts_of(db, c, input, None)
}
/// Whom the DSP can text from `line`, or those found by a name or number typed, `q`, and the
/// numbers that line blocked.
pub fn text_contacts_of(
    db: &Store,
    c: &Member,
    input: &Input,
    line: Option<Line>,
) -> Result<Reply> {
    let line = line.map(|line| line.id);
    served(c, line)?;
    v::fields(&input.query, &["q"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let typed = (!typed.is_empty()).then_some(typed);
    Reply::of(&texts::contacts::contacts(
        db,
        c.state,
        c.dsp_id(),
        line,
        typed,
    )?)
}
/// A text's picture or file, as the phone side keeps it, kept by the browser at its version, `v`.
pub(super) async fn text_file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    text_file_of(state, input, access, None).await
}
/// A picture or file of a text of `line`, as `text_file` answers it: another line's isn't found.
pub async fn text_file_of(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    line: Option<Line<'static>>,
) -> Result<Reply> {
    text_shown(state, input, access, line, false).await
}
/// A text's picture's preview, as `text_file` answers it, the picture itself when it has none; or
/// the picture of the first frame of a text's video kept on its phone.
pub(super) async fn text_file_preview(
    state: Arc<State>,
    input: Input,
    access: Dsp,
) -> Result<Reply> {
    text_file_preview_of(state, input, access, None).await
}
/// The preview of a picture or video of a text of `line`, as `text_file_preview` answers it.
pub async fn text_file_preview_of(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    line: Option<Line<'static>>,
) -> Result<Reply> {
    text_shown(state, input, access, line, true).await
}
/// A text's picture or file of `line`, or its `preview`.
async fn text_shown(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    line: Option<Line<'static>>,
    preview: bool,
) -> Result<Reply> {
    let (asked, access) = (input.clone(), Access::of(access, line));
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &["v"])?;
    let version = optional(&input.query, "v", |q, key| v::text(q, key, 1, 64))?;
    let wanted = texting_id(input.param("id"))?;
    texts::reading::file(&state, &c, access, &wanted, version, preview).await
}
/// Asks the phone of a text's video it keeps for its bytes, to play it.
pub(super) async fn fetch_text_file(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    fetch_text_file_of(state, input, access, None).await
}
/// Asks the phone of a video of a text of `line` for its bytes, as `fetch_text_file` does.
pub async fn fetch_text_file_of(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    line: Option<Line<'static>>,
) -> Result<Reply> {
    let (asked, access) = (input.clone(), Access::of(access, line));
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.query, &[])?;
    v::fields(&input.body, &[])?;
    let wanted = texting_id(input.param("id"))?;
    texts::reading::fetch(&state, &c, access, &wanted).await
}
/// Keeps a picture added for a text not sent yet, as the phone side keeps it: its `name` and
/// `type` in the query.
pub(super) async fn add_text_photo(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    photo: Upload,
) -> Result<Reply> {
    add_text_photo_of(state, input, access, photo, None).await
}
/// Keeps a picture added for a text of `line` not sent yet, as `add_text_photo` keeps it: of no
/// line until it's sent, its adder's alone.
pub async fn add_text_photo_of(
    state: Arc<State>,
    input: Input,
    access: Dsp,
    photo: Upload,
    line: Option<Line<'static>>,
) -> Result<Reply> {
    let (asked, access) = (input.clone(), Access::of(access, line));
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
