//! Messaging a group: whom one can go to, making one and messaging it again, reading it and its
//! people, and Done for everyone listed under it.
use super::{before, file_id, id, photos, slack_id};
use crate::backend::{
    compose,
    groups::{self, choices, send},
};
use dispatch_core::{
    Error, Result, State,
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, optional_text},
        route::{Dsp, Grant, Member},
    },
};
use serde_json::Value;
use std::sync::Arc;

/// Whom a group message can go to, and the groups to choose from, today being the DSP's own day.
pub(super) fn group_choices(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&choices::choices(db, c.state, c.dsp_id(), &c.dsp.timezone)?)
}
/// A group and its latest messages and answers, or those before `before`, `<time>:<id>`.
pub(super) fn group(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
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
pub(super) fn group_people(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    let group = id(input.param("id"), "grp")?;
    Reply::of(&groups::people(db, c.state, c.dsp_id(), &group)?)
}
/// Marks done every conversation listed under a group that waits.
pub(super) fn group_done(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.body, &[])?;
    let group = id(input.param("id"), "grp")?;
    Reply::of(&groups::done(db, c.state, c.dsp_id(), &group, c.actor())?)
}

/// Messages a group: its `name`, its `people`, each `teamId` and `userId`, and the `text` and the
/// `photos` added for it.
pub(super) async fn create_group(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["name", "people", "text", "photos"])?;
    let name = optional_text(&input.body, "name", send::NAME_MOST * 4)?;
    let people = input
        .body
        .get("people")
        .and_then(Value::as_array)
        .filter(|people| people.len() <= send::PEOPLE_MOST)
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
    let text = optional_text(&input.body, "text", compose::SLACK.text * 2)?;
    let photos = photos(&input.body, file_id)?;
    let group = send::create(&state, &c, access, name, &people, text, &photos).await?;
    send::send_soon(&state, &c.dsp.id);
    Reply::of_status(&group, 201)
}
/// Sends a group another message, to the same people: its `text` and the `photos` added for it.
pub(super) async fn message_group(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
    let asked = input.clone();
    let c = state.read(move |db| access.authorize(db, &asked)).await?;
    v::fields(&input.body, &["text", "photos"])?;
    let group = id(input.param("id"), "grp")?;
    let text = optional_text(&input.body, "text", compose::SLACK.text * 2)?;
    let photos = photos(&input.body, file_id)?;
    let entry = send::again(&state, &c, access, &group, text, &photos).await?;
    send::send_soon(&state, &c.dsp.id);
    Reply::of_status(&entry, 201)
}
