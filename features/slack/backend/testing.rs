//! What tests use of Slack, its own and, behind its `testing` feature, a dependant's: connecting
//! one of a DSP's fixture workspaces without installing the app there, switching an integration
//! there, and disconnecting it.
use super::{
    bot::Bot,
    connection,
    slack::Tokens,
    storage::{SlackStore, Workspace},
};
use dispatch_core::{State, db::now};
use std::sync::Arc;

/// Connects the DSP's workspace `team`, its admin having let the bot do what `scopes` say.
pub async fn connect(state: &Arc<State>, dsp: &str, team: &str, scopes: &[&str]) -> Bot {
    let workspace = Workspace {
        team: team.into(),
        name: team.into(),
        broken: false,
        scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        bot_user: "U0FIXTUREBOT".into(),
        app: "A0FIXTURE".into(),
        connected_by: None,
        connected_at: "2026-01-01T00:00:00Z".into(),
        label: None,
    };
    let tokens = Tokens {
        access: format!("fixture-access:{team}:test"),
        refresh: None,
        expires_at: Some(now() + 3_600_000),
    };
    let of = dsp.to_owned();
    state
        .run(move |db| db.save_slack_workspace(&of, &workspace, &tokens))
        .await
        .unwrap();
    Bot::new(state, dsp, team)
}

/// Forgets the DSP's workspace `team`, as when Slack says the app was removed there: what Slack
/// and each integration keep of it goes.
pub async fn disconnect(state: &Arc<State>, dsp: &str, team: &str) {
    let (dsp, team) = (dsp.to_owned(), team.to_owned());
    state
        .run(move |db| connection::removed(db, &dsp, &team))
        .await
        .unwrap();
}
/// Switches the integration `integration` on or off in the DSP's workspace `team`, as someone
/// who manages Slack does in its Integrations.
pub async fn switch(state: &Arc<State>, dsp: &str, team: &str, integration: &'static str, on: bool) {
    let (dsp, team) = (dsp.to_owned(), team.to_owned());
    state
        .run(move |db| match on {
            true => db.turn_on_slack_integration(&dsp, &team, integration, "user_test"),
            false => db.turn_off_slack_integration(&dsp, &team, integration, "user_test"),
        })
        .await
        .unwrap();
}
