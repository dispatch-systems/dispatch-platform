//! What other features do through Slack, each declared in core's integrations slot with Slack as
//! its connection: the permissions and events it needs, which `app` asks for beside Slack's own,
//! and what it does with what it hears. Someone who manages Slack switches each on or off for
//! each of the DSP's workspaces, in the workspace's settings; one hears of what happens there
//! while it's on and its feature is on for the DSP. One that hears of what people write to the
//! app directly (`message.im`) takes it from Slack, which otherwise answers it itself. A new
//! one needs nothing here.
use super::{bot::Bot, connection, storage::SlackStore};
use crate::api::types::SlackAccount;
use dispatch_core::{
    Error, Result,
    accounts::Context,
    db::Store,
    ensure,
    manifest::{
        Feature,
        integrations::{Heard, Integration},
        registry,
    },
};
use serde_json::Value;

/// The connection an integration names to work through Slack: Slack's own name.
const SLACK: &str = "slack";

/// Every feature's integration through Slack, with its feature, in the registry's order.
pub fn declared() -> impl Iterator<Item = (&'static Feature, &'static Integration)> {
    registry().integrations(SLACK)
}

/// Switches the integration `id` on or off in the DSP's workspace `team`, for a member who
/// manages Slack, and records it in the activity log as `<integration> in <workspace>`. An
/// integration whose feature the DSP doesn't have is as good as none.
pub fn switch(db: &Store, c: &Context, team: &str, id: &str, on: bool) -> Result<SlackAccount> {
    let integration = declared()
        .find(|(feature, integration)| integration.id == id && c.has(feature.name))
        .map(|(_, integration)| integration)
        .ok_or_else(|| Error::new("slack_integration_not_found", 404))?;
    let workspace = db.slack_workspace(&c.dsp.id, team)?;
    let workspace = workspace.ok_or_else(|| Error::new("slack_workspace_not_found", 404))?;
    ensure(!workspace.broken, "slack_connection_broken", 409)?;
    let switched = if on {
        db.turn_on_slack_integration(&c.dsp.id, team, id, &c.auth.user.id)?
    } else {
        db.turn_off_slack_integration(&c.dsp.id, team, id, &c.auth.user.id)?
    };
    if switched {
        let named = workspace.label.as_deref().unwrap_or(&workspace.name);
        let action = if on {
            "slack.integration_on"
        } else {
            "slack.integration_off"
        };
        c.audit(db, action, &format!("{} in {named}", integration.label))?;
    }
    connection::account(db, &c.dsp.id)
}

/// Hands what Slack told the bot of, an event of the kind `kind`, to each integration that
/// hears of it and is on in the bot's workspace, while its feature is on for the DSP: whether
/// any did. Each runs on its own, and logs its own failures.
pub async fn heard(bot: &Bot, kind: &str, event: &Value) -> Result<bool> {
    let listening: Vec<_> = declared()
        .filter(|(_, integration)| integration.hears.contains(&kind))
        .collect();
    if listening.is_empty() {
        return Ok(false);
    }
    let (dsp, team) = (bot.dsp.clone(), bot.team.clone());
    let on = bot
        .state
        .read(move |db| {
            let on = db.slack_integrations(&dsp, &team)?;
            let mut hearing = Vec::new();
            for (feature, integration) in listening {
                if on.iter().any(|id| id == integration.id)
                    && db.feature_enabled(&dsp, feature.name)?
                {
                    hearing.push(integration);
                }
            }
            Ok(hearing)
        })
        .await?;
    let any = !on.is_empty();
    for heard in on.into_iter().filter_map(|integration| integration.heard) {
        let told = Heard {
            dsp: bot.dsp.clone(),
            account: bot.team.clone(),
            kind: kind.to_owned(),
            event: event.clone(),
        };
        tokio::spawn(heard(bot.state.clone(), told));
    }
    Ok(any)
}

#[cfg(test)]
#[path = "../tests/backend/integrations.rs"]
mod tests;
