//! The list: its conversations of every section, those waiting for the sidebar and alerts, what
//! changes in it as a page waits, and its conversations found by what was typed or read a page at
//! a time, each section as `sections` describes it.
use super::{before, team};
use crate::backend::{
    inbox, live,
    model::ConversationKind,
    sections::{self, Asked, Query},
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
use std::{sync::Arc, time::Duration};

/// How long a page waiting on changes waits before it's answered with none.
const WAIT: Duration = Duration::from_secs(20);

/// The conversations waiting, and the latest answered of the workspace `team` or every one.
pub(super) fn overview(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["team"])?;
    let team = team(&input.query)?;
    Reply::of(&inbox::overview(
        db,
        c.state,
        c.dsp_id(),
        c.actor(),
        team.as_deref(),
    )?)
}
pub(super) fn waiting(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &[])?;
    Reply::of(&inbox::waiting(db, c.state, c.dsp_id())?)
}
/// Conversations found by a name typed, `q`, or the answered ones before `before`, of the section
/// listing `kind`: Slack's by default, of the workspace `team` or every one; the numbers'
/// (`kind=number`), or the work ones found by a name or digits typed (`kind=work`).
pub(super) fn found(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["q", "team", "before", "kind"])?;
    let typed = optional_text(&input.query, "q", 100)?.trim();
    let team = team(&input.query)?;
    let kind = match optional_text(&input.query, "kind", 10)? {
        "" => ConversationKind::Slack,
        // Slack's is found by default, never by name.
        named => ConversationKind::parse(named)
            .filter(|kind| *kind != ConversationKind::Slack)
            .ok_or_else(|| Error::new("invalid_input", 400))?,
    };
    // Typed, or else read on from the last listed.
    let before = match typed {
        "" => Some(before(&input.query)?.ok_or_else(|| Error::new("invalid_input", 400))?),
        _ => None,
    };
    let asked = match &before {
        Some((at, id)) => Asked::Before((*at, id)),
        None => Asked::Typed(typed),
    };
    let query = Query {
        team: team.as_deref(),
        asked,
    };
    Reply::of(&sections::found(db, c.state, c.dsp_id(), kind, &query)?)
}

/// What a page waiting on changes is answered with: what changed since the change `after`, and
/// who is writing, once either differs from what it has (`typing`, the version of that), or
/// twenty seconds pass.
pub(super) async fn updates(state: Arc<State>, input: Input, access: Dsp) -> Result<Reply> {
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
                inbox::updates(db, &shared, &c.dsp.id, after, &c.auth.user.id)
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
