//! Fixture mode's Slack, in-process, so previews and tests never reach Slack. Installing comes
//! straight back with a code naming the workspace, `fixture:<team>`; its tokens name their
//! workspace too, and one whose ID holds `GONE` is refused, as a workspace that removed the app
//! is. Each workspace has the same channels, the bot in `general` and in the private
//! `managers`, and what it joins and leaves is kept for the server's run. Requests it sends
//! are signed with a secret the tests share.
use super::slack::{Installed, Tokens};
use super::{app::SCOPES, storage::SlackStore};
use dispatch_core::{
    Error, Result,
    db::{Store, now},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

/// The secret its requests are signed with, which tests sign theirs with too.
pub const SIGNING_SECRET: &str = "fixture-signing-secret";
/// Its workspaces, which installs take in turn: each the first that no DSP has yet.
const TEAMS: &[(&str, &str)] = &[
    ("T0FIXTURE1", "Operations"),
    ("T0FIXTURE2", "Drivers"),
    ("T0FIXTURE3", "Managers"),
];
const GONE: &str = "GONE";
/// Each workspace's channels: its ID, its name, whether it's private, and whether the bot
/// starts in it.
const CHANNELS: &[(&str, &str, bool, bool)] = &[
    ("C0FIXGENERAL", "general", false, true),
    ("C0FIXDISPATCH", "dispatch", false, false),
    ("C0FIXPHOTOS", "photos", false, false),
    ("C0FIXRANDOM", "random", false, false),
    ("G0FIXMANAGERS", "managers", true, true),
];
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
/// Slack's answer to a call with `args`: the workspace for `team.info`; the channels the bot
/// sees, joins, leaves or is asked of for `conversations.*`; and `ok` to anything else.
pub fn call(token: &str, method: &str, args: &Value) -> Result<Value> {
    let team = workspace(token, "fixture-access:")?;
    let asked = args["channel"].as_str().unwrap_or("");
    Ok(match method {
        "team.info" => json!({"ok": true, "team": {"id": team, "name": name(team)}}),
        "conversations.list" => {
            let seen: Vec<Value> = CHANNELS
                .iter()
                .map(|(id, ..)| channel(team, id))
                .filter(|channel| channel["is_private"] == false || channel["is_member"] == true)
                .collect();
            json!({"ok": true, "channels": seen, "response_metadata": {"next_cursor": ""}})
        }
        "conversations.info" => json!({"ok": true, "channel": seen(team, asked)?}),
        "conversations.join" => {
            let found = seen(team, asked)?;
            if found["is_private"] == true {
                return Err(Error::new("slack_refused", 502));
            }
            joined(team, asked, true);
            json!({"ok": true, "channel": channel(team, asked)})
        }
        "conversations.leave" => {
            seen(team, asked)?;
            joined(team, asked, false);
            json!({"ok": true})
        }
        _ => json!({"ok": true}),
    })
}

/// Whether the bot is in each channel it joined or left, by workspace and channel.
static MEMBERS: LazyLock<Mutex<HashMap<(String, String), bool>>> = LazyLock::new(Default::default);
fn joined(team: &str, id: &str, member: bool) {
    MEMBERS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert((team.to_owned(), id.to_owned()), member);
}
/// The workspace's channel `id`, as Slack describes it to the bot.
fn channel(team: &str, id: &str) -> Value {
    let Some((_, name, private, starts)) = CHANNELS.iter().find(|(known, ..)| *known == id) else {
        return Value::Null;
    };
    let member = MEMBERS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&(team.to_owned(), id.to_owned()))
        .copied()
        .unwrap_or(*starts);
    json!({"id": id, "name": name, "is_private": private, "is_member": member})
}
/// The channel `id`, if the bot sees it: any public one, and a private one it's in.
fn seen(team: &str, id: &str) -> Result<Value> {
    let found = channel(team, id);
    let sees = found["is_private"] == false || found["is_member"] == true;
    if found.is_null() || !sees {
        return Err(Error::new("slack_refused", 502));
    }
    Ok(found)
}

fn name(team: &str) -> String {
    TEAMS.iter().find(|(id, _)| *id == team).map_or_else(
        || format!("Workspace {team}"),
        |(_, name)| (*name).to_owned(),
    )
}
/// New tokens for the workspace, `fixture-access:<team>:<turn>`, so each renewal gives others.
fn tokens(team: &str) -> Tokens {
    static TURNS: AtomicU64 = AtomicU64::new(0);
    let turn = TURNS.fetch_add(1, Ordering::Relaxed);
    Tokens {
        access: format!("fixture-access:{team}:{turn}"),
        refresh: Some(format!("fixture-refresh:{team}:{turn}")),
        expires_at: Some(now() + LASTS_MS),
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
