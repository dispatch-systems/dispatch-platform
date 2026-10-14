//! The app's bot in one of a DSP's workspaces: what everything that acts in Slack holds, and
//! its only way to call Slack. It calls any of Slack's API methods there with the workspace's
//! token. Slack's tokens last twelve hours: the bot renews one before using it once less than
//! an hour and five minutes is left, so the hourly check renews a token before it ends, with a
//! refresh token Slack honours once, and keeps the new pair at once. One renewal runs at a time
//! for each workspace, so two never spend the same refresh token.
use super::{
    connection,
    slack::{Slack, Tokens},
    storage::SlackStore,
};
use dispatch_core::{Error, Result, State, db::now};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

/// How near its end a token is renewed before it's used: within the hourly check's hour, and
/// five minutes more, so the check renews one before it ends.
const RENEW_WITHIN_MS: i64 = 65 * 60 * 1000;

/// The bot in the DSP's workspace `team`.
pub struct Bot {
    pub state: Arc<State>,
    pub dsp: String,
    pub team: String,
}
impl Bot {
    pub fn new(state: &Arc<State>, dsp: &str, team: &str) -> Self {
        Self {
            state: state.clone(),
            dsp: dsp.to_owned(),
            team: team.to_owned(),
        }
    }
    /// The bot in the workspace `team`, for the DSP it belongs to, while Dispatch acts there:
    /// the DSP active, Slack on for it, and the workspace connected and working.
    pub async fn serving(state: &Arc<State>, team: &str) -> Result<Option<Self>> {
        let of = team.to_owned();
        let dsp = state
            .read(move |db| {
                let Some(dsp) = db.slack_team_dsp(&of)? else {
                    return Ok(None);
                };
                let working = db
                    .slack_workspace(&dsp, &of)?
                    .is_some_and(|workspace| !workspace.broken);
                Ok((working && connection::open(db, &dsp)?).then_some(dsp))
            })
            .await?;
        Ok(dsp.map(|dsp| Self::new(state, &dsp, team)))
    }

    /// Calls Slack's API method `method` with `args`: Slack's answer. A token Slack says
    /// expired is renewed and the call made again, once. Slack no longer honouring the
    /// workspace's tokens breaks it, until someone reconnects it.
    pub async fn call(&self, method: &str, args: &Value) -> Result<Value> {
        let slack = Slack::of(&self.state.config)?;
        let token = self.token().await?;
        let answer = match slack.call(&token, method, args).await {
            Err(error) if error.code == "slack_token_expired" => {
                slack
                    .call(&self.instead_of(&token).await?, method, args)
                    .await
            }
            answer => answer,
        };
        if let Err(error) = &answer
            && error.code == "slack_connection_broken"
        {
            self.broken().await?;
        }
        answer
    }

    /// The workspace's access token, renewed first when it nears its end.
    pub async fn token(&self) -> Result<String> {
        self.renewed(|kept| {
            kept.expires_at
                .is_some_and(|expires| expires - now() <= RENEW_WITHIN_MS)
        })
        .await
    }
    /// A token in place of `refused`, which Slack said expired: renewed, unless another call
    /// renewed it meanwhile.
    async fn instead_of(&self, refused: &str) -> Result<String> {
        self.renewed(|kept| kept.access == refused).await
    }
    /// The workspace's access token, renewed first when `due` says so of the kept tokens.
    /// Slack refusing to renew it breaks the workspace.
    async fn renewed(&self, due: impl FnOnce(&Tokens) -> bool) -> Result<String> {
        let lock = renewing(&self.dsp, &self.team);
        let _renewing = lock.lock().await;
        let (dsp, team) = (self.dsp.clone(), self.team.clone());
        let kept = self
            .state
            .read(move |db| db.slack_tokens(&dsp, &team))
            .await?
            .ok_or_else(|| Error::new("slack_not_connected", 404))?;
        // A token without a refresh token never expires: the app's rotation is off.
        let Some(refresh) = kept.refresh.clone().filter(|_| due(&kept)) else {
            return Ok(kept.access);
        };
        match Slack::of(&self.state.config)?.refresh(&refresh).await {
            Ok(tokens) => {
                let access = tokens.access.clone();
                let (dsp, team) = (self.dsp.clone(), self.team.clone());
                self.state
                    .run(move |db| db.save_slack_tokens(&dsp, &team, &tokens))
                    .await?;
                Ok(access)
            }
            Err(error) => {
                if error.code == "slack_connection_broken" {
                    self.broken().await?;
                }
                Err(error)
            }
        }
    }

    async fn broken(&self) -> Result<()> {
        let (dsp, team) = (self.dsp.clone(), self.team.clone());
        self.state
            .run(move |db| connection::broken(db, &dsp, &team))
            .await
    }
}

/// The lock on each workspace's renewals.
fn renewing(dsp: &str, team: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    LOCKS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(format!("{dsp}:{team}"))
        .or_default()
        .clone()
}

#[cfg(test)]
#[path = "../tests/backend/bot.rs"]
mod tests;
