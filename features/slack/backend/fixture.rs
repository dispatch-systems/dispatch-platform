//! Fixture mode's Slack, in-process, so previews and tests never reach Slack. Installing comes
//! straight back with a code naming the workspace, `fixture:<team>`; its tokens name their
//! workspace too, and one whose ID holds `GONE` is refused, as a workspace that removed the app
//! is. Requests it sends are signed with a secret the tests share.
use super::slack::{Installed, Tokens};
use super::{app::SCOPES, storage::SlackStore};
use dispatch_core::{
    Error, Result,
    db::{Store, now},
};
use serde_json::{Value, json};

/// The secret its requests are signed with, which tests sign theirs with too.
pub const SIGNING_SECRET: &str = "fixture-signing-secret";
/// Its workspaces, which installs take in turn: each the first that no DSP has yet.
const TEAMS: &[(&str, &str)] = &[
    ("T0FIXTURE1", "Operations"),
    ("T0FIXTURE2", "Drivers"),
    ("T0FIXTURE3", "Managers"),
];
const GONE: &str = "GONE";
/// How long its tokens last, as Slack's rotating tokens do.
const LASTS_MS: i64 = 12 * 60 * 60 * 1000;

/// The workspace an install goes to when it names none: the first no DSP has yet.
pub fn free_team(db: &Store) -> Result<String> {
    for (team, _) in TEAMS {
        if db.slack_team_dsp(team)?.is_none() {
            return Ok((*team).to_owned());
        }
    }
    Err(Error::new("slack_workspace_taken", 409))
}

pub fn install(code: &str) -> Result<Installed> {
    let team = code
        .strip_prefix("fixture:")
        .filter(|team| {
            !team.is_empty()
                && team
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .ok_or_else(|| Error::new("slack_install_failed", 502))?;
    Ok(Installed {
        team: team.to_owned(),
        name: name(team),
        bot_user: "U0FIXTUREBOT".into(),
        app: "A0FIXTURE".into(),
        scopes: SCOPES.iter().map(|scope| (*scope).to_owned()).collect(),
        tokens: tokens(team),
    })
}
pub fn refresh(refresh: &str) -> Result<Tokens> {
    Ok(tokens(workspace(refresh, "fixture-refresh:")?))
}
/// Slack's answer to a call: the workspace for `team.info`, and `ok` to anything else.
pub fn call(token: &str, method: &str) -> Result<Value> {
    let team = workspace(token, "fixture-access:")?;
    Ok(match method {
        "team.info" => json!({"ok": true, "team": {"id": team, "name": name(team)}}),
        _ => json!({"ok": true}),
    })
}

fn name(team: &str) -> String {
    TEAMS.iter().find(|(id, _)| *id == team).map_or_else(
        || format!("Workspace {team}"),
        |(_, name)| (*name).to_owned(),
    )
}
/// New tokens for the workspace, `fixture-access:<team>:<turn>`, so each renewal gives others.
fn tokens(team: &str) -> Tokens {
    let turn = now();
    Tokens {
        access: format!("fixture-access:{team}:{turn}"),
        refresh: Some(format!("fixture-refresh:{team}:{turn}")),
        expires_at: Some(turn + LASTS_MS),
    }
}
/// The workspace a token of `kind` names, unless it is one that removed the app.
fn workspace<'a>(token: &'a str, kind: &str) -> Result<&'a str> {
    token
        .strip_prefix(kind)
        .and_then(|rest| rest.split(':').next())
        .filter(|team| !team.is_empty() && !team.contains(GONE))
        .ok_or_else(|| Error::new("slack_connection_broken", 409))
}
