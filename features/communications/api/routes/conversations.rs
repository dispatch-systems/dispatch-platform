//! A conversation of either kind, by its ID, through the backend's `dispatcher`: reading it, its
//! profile, sending in it, Done and who's writing; and Slack's own, opening a direct message and
//! whom the DSP can message there.
use super::{PLACE_MOST, photo_id, photos, slack_id, team};
use crate::backend::{compose, contacts, dispatcher, live, model::Reference, reply};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text},
        route::{Dsp, Grant, Member},
    },
};
use std::sync::Arc;

/// A conversation and its latest messages, or those before the one at `before`.
pub(super) fn conversation(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["before"])?;
    let reference = Reference::parse(input.param("id"))?;
    let before = optional_text(&input.query, "before", PLACE_MOST)?;
    let before = (!before.is_empty()).then_some(before);
    Reply::of(&dispatcher::opened(
        db,
        c.state,
        c.dsp_id(),
        &reference,
        before,
    )?)
}
/// What the profile beside a conversation shows of whom it's with, a member of a workspace or a
/// driver: behind `communications.use` alone, as it reads what other features know of them for
/// the page.
pub(super) fn profile(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let reference = Reference::parse(input.param("id"))?;
    Reply::of(&dispatcher::profile(db, c.state, c.dsp_id(), &reference)?)
}
pub(super) fn done(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let reference = Reference::parse(input.param("id"))?;
    Reply::of(&dispatcher::done(
        db,
        c.state,
        c.dsp_id(),
        &reference,
        c.actor(),
    )?)
}
/// Sends a message in a conversation: its `text`, and the `photos` added for it, by text the
/// pictures added through texting (`texts/uploads`), in Slack those Communications keeps.
pub(super) async fn send(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let reference = Reference::parse(input.param("id"))?;
    let of = reference.clone();
    let (c, limits) = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            let limits = compose::of(db, &c.dsp.id, &of)?
                .ok_or_else(|| Error::new("communications_texting_off", 409))?;
            Ok((c, limits))
        })
        .await?;
    v::fields(&input.body, &["text", "photos"])?;
    let text = optional_text(&input.body, "text", limits.read())?;
    let photos = photos(&input.body, photo_id(&reference))?;
    let message = dispatcher::send(&state, &c, access, &reference, text, &photos).await?;
    Reply::of_status(&message, 201)
}
/// Notes that the member is writing in a conversation, for the next few seconds.
pub(super) async fn typing(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let reference = Reference::parse(input.param("id"))?;
    let of = reference.clone();
    let c = state
        .read(move |db| {
            let c = access.authorize(db, &asked)?;
            dispatcher::found(db, &c.dsp.id, &of)?;
            Ok(c)
        })
        .await?;
    v::fields(&input.body, &[])?;
    live::writing(&state, &c.dsp.id, reference.id(), &c.auth.user);
    Ok(Reply::ok())
}

/// Opens a direct message with a member of one of the DSP's workspaces: `teamId` and `userId`.
pub(super) async fn open(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["teamId", "userId"])?;
    let team = slack_id(v::text(&input.body, "teamId", 2, 32)?, b"T")?;
    let user = slack_id(v::text(&input.body, "userId", 2, 32)?, b"UW")?;
    Reply::of(&reply::open(&state, &c, access, &team, &user).await?)
}
/// Whom the DSP can message, in the workspace `team` or every one, or those found by a name
/// typed, `q`.
pub(super) fn listed(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
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
