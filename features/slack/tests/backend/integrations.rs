//! What other features do through Slack: the app asks for what their integrations need, an
//! integration hears of what happens in a workspace only while it's on there and its feature is
//! on for the DSP, Slack knowing whether any did, and it clears what it keeps of a workspace
//! Slack forgets.
use super::*;
use crate::backend::{
    app,
    storage::SlackStore,
    support::{self, LISTENING, STARTING},
};
use dispatch_core::{State, testing};
use serde_json::json;
use std::sync::Arc;

const TEAM: &str = "T0FIXTURE1";

/// A platform with one DSP, which has the workspace `TEAM`, and the bot there.
async fn connected() -> (impl Drop, Arc<State>, Bot) {
    let (root, state, dsp) = support::platform();
    let bot = support::connect(&state, &dsp, TEAM, &app::scopes()).await;
    (root, state, bot)
}

#[test]
fn the_app_asks_for_what_its_integrations_need_after_its_own() {
    support::install();
    let scopes = app::scopes();
    assert_eq!(&scopes[..app::SCOPES.len()], app::SCOPES);
    assert!(LISTENING.needs.iter().all(|need| scopes.contains(need)));
    let events = app::events();
    assert_eq!(&events[..app::EVENTS.len()], app::EVENTS);
    assert!(LISTENING.hears.iter().all(|kind| events.contains(kind)));
}

#[tokio::test]
async fn an_integration_hears_of_its_events_only_while_on_in_the_workspace_and_for_the_dsp() {
    let (_root, state, bot) = connected().await;
    let dsp = bot.dsp.clone();
    let shared = json!({"type": "file_shared", "file_id": "F0FIXTURE1"});
    let hear = |kind: &'static str| {
        let (bot, event) = (&bot, shared.clone());
        async move { heard(bot, kind, &event).await.unwrap() }
    };
    let switch = |on: bool| {
        let (state, dsp) = (state.clone(), dsp.clone());
        async move {
            state
                .run(move |db| match on {
                    true => db.turn_on_slack_integration(&dsp, TEAM, LISTENING.id, "U1"),
                    false => db.turn_off_slack_integration(&dsp, TEAM, LISTENING.id, "U1"),
                })
                .await
                .unwrap()
        }
    };
    let feature = |on: bool| {
        let (state, dsp) = (state.clone(), dsp.clone());
        async move {
            state
                .run(move |db| {
                    let owner = testing::a_user(db);
                    db.set_feature(&dsp, "listening", on, &owner).map(drop)
                })
                .await
                .unwrap()
        }
    };

    // Its feature on for the DSP, but off in the workspace, it hears of nothing, and Slack is
    // told none heard.
    feature(true).await;
    assert!(!hear("file_shared").await);
    assert!(support::heard(&dsp).is_empty());
    // On there, it hears of what it listens for, and nothing else.
    assert!(switch(true).await);
    assert!(hear("file_shared").await);
    assert!(!hear("team_join").await);
    let once = vec![(TEAM.to_owned(), "file_shared".to_owned())];
    assert_eq!(support::heard(&dsp), once);
    // Its feature off for the DSP, it hears of nothing more, though it's on in the workspace.
    feature(false).await;
    assert!(!hear("file_shared").await);
    assert_eq!(support::heard(&dsp), once);
    // Its feature back on, but switched off in the workspace: still nothing.
    feature(true).await;
    assert!(switch(false).await);
    assert!(!hear("file_shared").await);
    assert_eq!(support::heard(&dsp), once);
}

#[tokio::test]
async fn an_integration_clears_what_it_keeps_of_a_workspace_slack_forgets_even_while_off() {
    let (_root, state, bot) = connected().await;
    let dsp = bot.dsp.clone();
    let of = dsp.clone();
    state
        .run(move |db| {
            let owner = testing::a_user(db);
            db.set_feature(&of, "listening", false, &owner)?;
            db.remove_slack_workspace(&of, TEAM)
        })
        .await
        .unwrap();
    assert_eq!(support::forgotten(&dsp), [TEAM]);
}

#[tokio::test]
async fn an_integration_that_starts_on_is_on_in_every_workspace_until_switched_off_there() {
    let (_root, state, bot) = connected().await;
    let dsp = bot.dsp.clone();
    let of = dsp.clone();
    let read = move |db: &dispatch_core::db::Store| {
        Ok((
            db.slack_integrations(&of, TEAM)?,
            db.slack_integration_on_since(&of, TEAM, STARTING.id)?,
        ))
    };
    // On from when the workspace was connected, 2026-01-01T00:00:00Z, the one that doesn't
    // start on off.
    let (on, since) = state.read(read.clone()).await.unwrap();
    assert_eq!(on, [STARTING.id]);
    assert_eq!(since, Some(1_767_225_600_000));
    // Switched off, it stays off, and isn't on since anything.
    let of = dsp.clone();
    let switched = state
        .run(move |db| db.turn_off_slack_integration(&of, TEAM, STARTING.id, "U1"))
        .await
        .unwrap();
    assert!(switched);
    assert_eq!(state.read(read.clone()).await.unwrap(), (vec![], None));
    // Switched on again, it's on since then.
    let before = dispatch_core::db::now();
    let of = dsp.clone();
    state
        .run(move |db| db.turn_on_slack_integration(&of, TEAM, STARTING.id, "U1"))
        .await
        .unwrap();
    let (on, since) = state.read(read.clone()).await.unwrap();
    assert_eq!(on, [STARTING.id]);
    // To the millisecond, as SQL reads the time back.
    assert!(since.unwrap() >= before - 1);
    // Switched off, then the workspace connected again: on again, as in a new one.
    let of = dsp.clone();
    state
        .run(move |db| {
            db.turn_off_slack_integration(&of, TEAM, STARTING.id, "U1")?;
            db.remove_slack_workspace(&of, TEAM).map(drop)
        })
        .await
        .unwrap();
    support::connect(&state, &dsp, TEAM, &app::scopes()).await;
    assert_eq!(state.read(read).await.unwrap().0, [STARTING.id]);
}
