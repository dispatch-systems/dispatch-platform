//! Fixture mode's Slack, in-process, so previews and tests never reach Slack. Installing comes
//! straight back with a code naming the workspace, `fixture:<team>`; its tokens name their
//! workspace too, and one whose ID holds `GONE` is refused, as a workspace that removed the app
//! is. Each workspace has the same channels, the bot in `general` and in the private
//! `managers`, and what it joins and leaves is kept for the server's run. Its members are the
//! demo's people, some by another name, with Slackbot, a bot, a guest and someone deactivated.
//! Its channels hold a few files, one too large to keep and a canvas, which download as bytes
//! made up to their size. Requests it sends are signed with a secret the tests share.
use super::slack::{Bytestream, Installed, Tokens};
use super::{app::scopes, storage::SlackStore};
use axum::body::Bytes;
use dispatch_core::{
    Error, Result,
    db::{Store, now},
};
use futures_util::stream;
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
/// A file in one of its channels.
struct File {
    id: &'static str,
    channel: &'static str,
    /// Who posted it, by their member ID.
    user: &'static str,
    name: &'static str,
    mime: &'static str,
    size: u64,
    /// How Slack keeps it: `hosted` for a file uploaded to Slack.
    mode: &'static str,
}
/// Its channels' files: a photo and a sheet, a video too large to keep, and a canvas.
const FILES: &[File] = &[
    File {
        id: "F0FIXVAN",
        channel: "C0FIXPHOTOS",
        user: "U0FIXOWNER",
        name: "Van 12 damage.jpg",
        mime: "image/jpeg",
        size: 2_048,
        mode: "hosted",
    },
    File {
        id: "F0FIXROUTE",
        channel: "C0FIXDISPATCH",
        user: "U0FIXCASEY",
        name: "Route 4 sheet.pdf",
        mime: "application/pdf",
        size: 4_096,
        mode: "hosted",
    },
    File {
        id: "F0FIXDASHCAM",
        channel: "C0FIXPHOTOS",
        user: "U0FIXCAM",
        name: "Dashcam.mov",
        mime: "video/quicktime",
        size: 150 * 1024 * 1024,
        mode: "hosted",
    },
    File {
        id: "F0FIXCANVAS",
        channel: "C0FIXPHOTOS",
        user: "U0FIXOWNER",
        name: "Write-ups",
        mime: "application/vnd.slack-docs",
        size: 0,
        mode: "canvas",
    },
];
/// Where a fixture file downloads from.
const DOWNLOAD: &str = "fixture-file:";
/// The file `id`, as `files.info` and `files.list` describe it, posted now.
fn file(id: &str) -> Option<Value> {
    let file = FILES.iter().find(|file| file.id == id)?;
    Some(json!({
        "id": file.id,
        "name": file.name,
        "title": file.name,
        "mimetype": file.mime,
        "size": file.size,
        "mode": file.mode,
        "user": file.user,
        "created": now() / 1000,
        "channels": [file.channel],
        "url_private_download": format!("{DOWNLOAD}{}", file.id),
    }))
}
/// The bytes of the file `url` names, made up to its size.
pub fn download(url: &str) -> Result<Bytestream> {
    let id = url.strip_prefix(DOWNLOAD).unwrap_or("");
    let file = FILES
        .iter()
        .find(|file| file.id == id)
        .ok_or_else(|| Error::new("slack_file_gone", 404))?;
    let bytes = Bytes::from(vec![b'x'; file.size as usize]);
    Ok(Box::pin(stream::iter([Ok(bytes)])))
}

/// What a fixture member is beside a person.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Person,
    Guest,
    Deactivated,
    Bot,
}
/// One of its members: the workspaces they're in, by their names there, their ID, name, the
/// name they go by when it's another, email address, phone number and what they are.
struct User {
    teams: &'static [&'static str],
    id: &'static str,
    name: &'static str,
    goes_by: &'static str,
    email: &'static str,
    phone: &'static str,
    kind: Kind,
}
const EVERY: &[&str] = &["Operations", "Drivers", "Managers"];
/// A person of the demo's in `teams`, by their name and email address.
const fn person(
    teams: &'static [&'static str],
    id: &'static str,
    name: &'static str,
    email: &'static str,
) -> User {
    User {
        teams,
        id,
        name,
        goes_by: "",
        email,
        phone: "",
        kind: Kind::Person,
    }
}
const USERS: &[User] = &[
    User {
        kind: Kind::Person,
        ..person(EVERY, "USLACKBOT", "Slackbot", "")
    },
    User {
        kind: Kind::Bot,
        ..person(EVERY, "U0FIXTUREBOT", "Dispatch Dev", "")
    },
    User {
        goes_by: "Morgan",
        ..person(
            &["Operations", "Managers"],
            "U0FIXOWNER",
            "Morgan Lane",
            "morgan.lane@example.com",
        )
    },
    person(
        &["Operations", "Drivers"],
        "U0FIXJORDAN",
        "Jordan Ellis",
        "jordan.ellis@example.com",
    ),
    User {
        goes_by: "Cam",
        ..person(
            &["Drivers"],
            "U0FIXCAM",
            "Cameron Hayes",
            "cam.hayes@example.com",
        )
    },
    User {
        phone: "(612) 555-0104",
        ..person(&["Drivers"], "U0FIXCASEY", "Casey Rivera", "")
    },
    User {
        kind: Kind::Guest,
        ..person(
            &["Drivers"],
            "U0FIXDREW",
            "Drew Sullivan",
            "drew.sullivan@example.com",
        )
    },
    User {
        kind: Kind::Deactivated,
        ..person(
            &["Drivers"],
            "U0FIXGONE",
            "Jesse Kline",
            "jesse.kline@example.com",
        )
    },
];
/// The members of the workspace named `workspace`, as `users.list` describes them.
fn users(workspace: &str) -> Vec<Value> {
    USERS
        .iter()
        .filter(|user| user.teams.contains(&workspace))
        .map(|user| {
            json!({
                "id": user.id,
                "deleted": user.kind == Kind::Deactivated,
                "is_bot": user.kind == Kind::Bot,
                "is_restricted": user.kind == Kind::Guest,
                "profile": {"real_name": user.name, "display_name": user.goes_by,
                    "email": user.email, "phone": user.phone},
            })
        })
        .collect()
}
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
        scopes: scopes().iter().map(|scope| (*scope).to_owned()).collect(),
        tokens: tokens(team),
    })
}
pub fn refresh(refresh: &str) -> Result<Tokens> {
    Ok(tokens(workspace(refresh, "fixture-refresh:")?))
}
/// Slack's answer to a call with `args`: the workspace for `team.info`; its members, on one
/// page, for `users.list`; the channels the bot sees, joins, leaves or is asked of for
/// `conversations.*`; a file, or a channel's files, for `files.*`; and `ok` to anything else.
pub fn call(token: &str, method: &str, args: &Value) -> Result<Value> {
    let team = workspace(token, "fixture-access:")?;
    let asked = args["channel"].as_str().unwrap_or("");
    Ok(match method {
        "team.info" => json!({"ok": true, "team": {"id": team, "name": name(team)}}),
        "users.list" => json!({
            "ok": true,
            "members": users(&name(team)),
            "response_metadata": {"next_cursor": ""},
        }),
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
        // Any of its members, as Slack describes someone in a channel shared across workspaces.
        "users.info" => {
            let asked = args["user"].as_str().unwrap_or("");
            let found = EVERY
                .iter()
                .flat_map(|workspace| users(workspace))
                .find(|user| user["id"] == asked)
                .ok_or_else(|| Error::new("slack_refused", 502))?;
            json!({"ok": true, "user": found})
        }
        "files.info" => {
            let asked = args["file"].as_str().unwrap_or("");
            let found = file(asked).ok_or_else(|| Error::new("slack_file_gone", 404))?;
            json!({"ok": true, "file": found})
        }
        "files.list" => {
            let listed: Vec<Value> = FILES
                .iter()
                .filter(|file| file.channel == asked)
                .filter_map(|listed| file(listed.id))
                .collect();
            json!({"ok": true, "files": listed, "paging": {"pages": 1}})
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
