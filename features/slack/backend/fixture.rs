//! Fixture mode's Slack, in-process, so previews and tests never reach Slack. Installing comes
//! straight back with a code naming the workspace, `fixture:<team>`; its tokens name their
//! workspace too, and one whose ID holds `GONE` is refused, as a workspace that removed the app
//! is. Each workspace has the same channels, the bot in `general` and in the private
//! `managers`, and what it joins and leaves is kept for the server's run. Its members are the
//! demo's people, some by another name, with Slackbot, a bot, a guest and someone deactivated.
//! Its channels hold a few files, one too large to keep, which download as bytes made up to
//! their size, and a canvas, which downloads as Slack's HTML of it, holding a photo that
//! downloads as a picture; a test edits a workspace's canvas with `edit_canvas`. Requests it
//! sends are signed with a secret the tests share.
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
        id: CANVAS,
        channel: "C0FIXPHOTOS",
        user: "U0FIXOWNER",
        name: "Write-ups",
        mime: "application/vnd.slack-docs",
        size: 0,
        mode: "quip",
    },
    // The photo in the canvas, in no channel of its own.
    File {
        id: CANVAS_PHOTO,
        channel: "",
        user: "U0FIXOWNER",
        name: "Write-up.png",
        mime: "image/png",
        size: PHOTO.len() as u64,
        mode: "hosted",
    },
];
/// Its canvas, and the photo in it.
pub const CANVAS: &str = "F0FIXCANVAS";
const CANVAS_PHOTO: &str = "F0FIXCANVASPHOTO";
/// The photo: a picture a pixel square.
const PHOTO: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];
/// The canvas as Slack's HTML has it, until a test edits it.
const WRITE_UPS: &str = concat!(
    r#"<div class="quip-canvas-content"><h1 id="temp:C:FIX1">Write-ups</h1>"#,
    r#"<p id="temp:C:FIX2" class="line">Verbal warnings for week 41, signed by "#,
    r#"<a>@U0FIXOWNER</a></p><div class='flexbox'><div class='flexbox-column'>"#,
    r#"<div data-section-style="11" class=''><img src='/collab-slack-blob/FIX9/F0FIXCANVASPHOTO"#,
    r#"?size_name=thumb_1024' id='temp:C:FIX3' alt="_SLACK_FILE_ALT_PLACEHOLDER_F0FIXCANVASPHOTO" "#,
    r#"width='64' height='50'>"#,
    "\u{200B}",
    r#"</img></div></div></div></div>"#,
);
/// Each workspace's canvas as a test edited it last, with when, in seconds.
static EDITED: LazyLock<Mutex<HashMap<String, (String, i64)>>> = LazyLock::new(Default::default);
/// When the canvas was last edited before a test edits it, in seconds: an hour before fixture
/// mode's Slack started.
static MADE: LazyLock<i64> = LazyLock::new(|| now() / 1000 - 60 * 60);
/// The workspace's canvas as Slack has it now, and when it was last edited, in seconds.
fn canvas(team: &str) -> (String, i64) {
    let edited = EDITED.lock().unwrap_or_else(|poison| poison.into_inner());
    edited
        .get(team)
        .cloned()
        .unwrap_or_else(|| (WRITE_UPS.to_owned(), *MADE))
}
/// Edits the workspace's canvas, as someone in Slack would, so it holds `html` from now on.
#[cfg(test)]
pub fn edit_canvas(team: &str, html: &str) {
    let mut edited = EDITED.lock().unwrap_or_else(|poison| poison.into_inner());
    let last = edited.get(team).map_or(*MADE, |(_, at)| *at);
    // Slack keeps edits to the second: each is a second after the last, at least.
    let at = (now() / 1000).max(last + 1);
    edited.insert(team.to_owned(), (html.to_owned(), at));
}
/// Where a fixture file downloads from: `fixture-file:<team>/<file>`.
const DOWNLOAD: &str = "fixture-file:";
/// The file `id`, as `files.info` and `files.list` describe it, posted now: a canvas with when
/// it was last edited, a picture with its size in pixels.
fn file(team: &str, id: &str) -> Option<Value> {
    let file = FILES.iter().find(|file| file.id == id)?;
    let mut described = json!({
        "id": file.id,
        "name": file.name,
        "title": file.name,
        "mimetype": file.mime,
        "filetype": if file.mode == "quip" { "quip" } else { "" },
        "size": file.size,
        "mode": file.mode,
        "user": file.user,
        "created": now() / 1000,
        "channels": [file.channel],
        "url_private_download": format!("{DOWNLOAD}{team}/{}", file.id),
    });
    if file.mode == "quip" {
        described["updated"] = json!(canvas(team).1);
    }
    if file.mime.starts_with("image/") {
        described["original_w"] = json!(1);
        described["original_h"] = json!(1);
    }
    Some(described)
}
/// The bytes of the file `url` names: Slack's HTML of a canvas, the photo's own, or any other's
/// made up to its size.
pub fn download(url: &str) -> Result<Bytestream> {
    let (team, id) = url
        .strip_prefix(DOWNLOAD)
        .and_then(|named| named.split_once('/'))
        .unwrap_or_default();
    let file = FILES
        .iter()
        .find(|file| file.id == id)
        .ok_or_else(|| Error::new("slack_file_gone", 404))?;
    let bytes = match file.id {
        CANVAS => Bytes::from(canvas(team).0),
        CANVAS_PHOTO => Bytes::from_static(PHOTO),
        _ => Bytes::from(vec![b'x'; file.size as usize]),
    };
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
            let found = file(team, asked).ok_or_else(|| Error::new("slack_file_gone", 404))?;
            json!({"ok": true, "file": found})
        }
        "files.list" => {
            let listed: Vec<Value> = FILES
                .iter()
                .filter(|file| file.channel == asked)
                .filter_map(|listed| file(team, listed.id))
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
