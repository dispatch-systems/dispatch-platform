//! What other features do through Slack: the app asks for what their integrations need, and an
//! integration hears of what happens in a workspace only while it's on there and its feature is
//! on for the DSP.
use super::*;
use crate::backend::{
    app,
    slack::Tokens,
    storage::Workspace,
    support::{self, LISTENING},
};
use dispatch_core::{State, db::now, testing};
use serde_json::json;
use std::sync::Arc;

const TEAM: &str = "T0FIXTURE1";

/// A platform with one DSP, which has the workspace `TEAM`, and the bot there.
async fn connected() -> (impl Drop, Arc<State>, Bot) {
    support::install();
    let (root, store, dsp) = testing::bootstrapped();
    let config = store.config.clone();
    drop(store);
    let state = State::new(config).unwrap();
    let workspace = Workspace {
        team: TEAM.into(),
        name: "Operations".into(),
        broken: false,
        scopes: app::scopes()
            .iter()
            .map(|scope| (*scope).to_owned())
            .collect(),
        bot_user: "U0FIXTUREBOT".into(),
        app: "A0FIXTURE".into(),
        connected_by: None,
        connected_at: "2026-10-16T12:00:00.000Z".into(),
        label: None,
    };
    let tokens = Tokens {
        access: format!("fixture-access:{TEAM}:test"),
        refresh: None,
        expires_at: Some(now() + 3_600_000),
    };
    let of = dsp.clone();
    state
        .run(move |db| db.save_slack_workspace(&of, &workspace, &tokens))
        .await
        .unwrap();
    let bot = Bot::new(&state, &dsp, TEAM);
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
                    false => db.turn_off_slack_integration(&dsp, TEAM, LISTENING.id),
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

    // Its feature on for the DSP, but off in the workspace, it hears of nothing.
    feature(true).await;
    hear("file_shared").await;
    assert!(support::heard(&dsp).is_empty());
    // On there, it hears of what it listens for, and nothing else.
    assert!(switch(true).await);
    hear("file_shared").await;
    hear("team_join").await;
    let once = vec![(TEAM.to_owned(), "file_shared".to_owned())];
    assert_eq!(support::heard(&dsp), once);
    // Its feature off for the DSP, it hears of nothing more, though it's on in the workspace.
    feature(false).await;
    hear("file_shared").await;
    assert_eq!(support::heard(&dsp), once);
    // Its feature back on, but switched off in the workspace: still nothing.
    feature(true).await;
    assert!(switch(false).await);
    hear("file_shared").await;
    assert_eq!(support::heard(&dsp), once);
}
