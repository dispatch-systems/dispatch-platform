//! Syncing one canvas: what Slack has of it now, made a page with its pictures in it, written
//! as a document where the DSP keeps its files, or over the one written before, wherever it's
//! been moved. A canvas that's gone from Slack leaves its document as it was. What can't be done
//! yet waits: while nothing keeps the DSP's files, or what keeps them isn't ready, or the
//! workspace's admin hasn't approved reading files. A failure is tried again, later each time.
use super::{
    page::{self, Picture},
    storage::{Canvas, Outcome, Status},
};
use crate::backend::{
    author,
    bot::Bot,
    slack::{Bytestream, Slack},
};
use dispatch_core::{
    Error, Result, State,
    db::now,
    manifest::services::files::{self, Author, Page},
};
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

/// How long a canvas waits while it can't be synced yet, in milliseconds.
const WAIT_MS: i64 = 15 * 60 * 1000;
/// How long a canvas waits after each failure, in milliseconds: given up after the last, until
/// it's edited again.
const RETRIES_MS: &[i64] = &[
    60 * 1000,
    5 * 60 * 1000,
    15 * 60 * 1000,
    60 * 60 * 1000,
    3 * 60 * 60 * 1000,
    6 * 60 * 60 * 1000,
    12 * 60 * 60 * 1000,
];
/// Where a canvas came from, as where it's kept says.
const FROM: &str = "Slack";
/// The most of Slack's HTML of a canvas read, in bytes: far more than any canvas holds.
const CANVAS_LIMIT: usize = 5 * 1024 * 1024;
/// The largest picture put in a page as it is, in bytes; a larger one goes in as Slack's
/// largest preview of it, sharp enough to read and small enough that a page of them fits.
const ORIGINAL_LIMIT: u64 = 2 * 1024 * 1024;
/// The most of one picture read, in bytes.
const PICTURE_LIMIT: usize = 8 * 1024 * 1024;
/// The kinds of picture a document shows as they are.
const SHOWN: [&str; 3] = ["image/png", "image/jpeg", "image/gif"];
/// Slack's previews of a picture, the largest first.
const PREVIEWS: [&str; 6] = [
    "thumb_1024",
    "thumb_960",
    "thumb_800",
    "thumb_720",
    "thumb_480",
    "thumb_360",
];

/// Tries to sync `canvas` for the DSP `dsp` into the folder `folder`, where a document new to
/// it goes: what it came to.
pub(super) async fn one(state: &Arc<State>, dsp: &str, canvas: &Canvas, folder: &str) -> Outcome {
    match attempt(state, dsp, canvas, folder).await {
        Ok(outcome) => outcome,
        Err(error) => failure(canvas, &error),
    }
}

async fn attempt(state: &Arc<State>, dsp: &str, canvas: &Canvas, folder: &str) -> Result<Outcome> {
    let at = dsp.to_owned();
    let Some(files) = state.read(move |db| files::for_dsp(db, &at)).await? else {
        return Ok(wait("slack_files_unavailable"));
    };
    if let Some(why) = files.unready(state, dsp).await? {
        return Ok(wait(&why));
    }
    let bot = Bot::new(state, dsp, &canvas.team);
    let info = match bot
        .call("files.info", &json!({"file": canvas.canvas}))
        .await
    {
        Err(error) if error.code == "slack_file_gone" => {
            return Ok(left(Status::Gone, "slack_file_gone"));
        }
        answer => answer?,
    };
    let file = &info["file"];
    // Slack keeps when it was last edited to the second.
    let edit = file["updated"].as_i64().unwrap_or(0) * 1000;
    let title = text(&file["title"])
        .or_else(|| text(&file["name"]))
        .unwrap_or_else(|| "Canvas".to_owned());
    let Some(url) = file["url_private_download"].as_str() else {
        return Ok(left(Status::Gone, "slack_file_gone"));
    };
    let slack = Slack::of(&state.config)?;
    let token = bot.token().await?;
    let html = read(
        slack.download(&token, url, "text/html").await?,
        CANVAS_LIMIT,
    )
    .await?;
    let html = String::from_utf8_lossy(&html).into_owned();
    // A token that may not read files is answered with Slack's sign-in page, a whole one.
    if html.contains("<html") {
        return Err(Error::new("slack_needs_update", 409));
    }
    let mut pictures = BTreeMap::new();
    for id in page::pictures(&html) {
        if let Some(picture) = picture(&bot, &slack, &token, &id).await? {
            pictures.insert(id, picture);
        }
    }
    let mut names = BTreeMap::new();
    for user in page::mentioned(&html) {
        let (_, shown) = author::of(&bot, Some(&user)).await?;
        names.insert(user, shown);
    }
    let html = page::page(&html, &pictures, &names);
    if let Some((id, _)) = &canvas.doc {
        // Written over as Slack changed it, by whoever Slack says changed it last.
        let changed = match file["last_editor"].as_str() {
            Some(user) => author::of(&bot, Some(user)).await?.0,
            None => Author::Named(FROM.to_owned()),
        };
        let page = Page {
            name: title.clone(),
            html: html.clone(),
            by: changed,
            from: FROM,
        };
        match files.rewrite(state, dsp, id, page).await {
            Ok(Some(saved)) => return Ok(synced(saved.id, saved.link, title, edit)),
            // Its document is gone: written anew, where new ones go.
            Ok(None) => {}
            Err(error) => return written(Err(error)),
        }
    }
    let (made_by, _) = author::of(&bot, file["user"].as_str()).await?;
    let page = Page {
        name: title.clone(),
        html,
        by: made_by,
        from: FROM,
    };
    written(
        files
            .write(state, dsp, folder, page)
            .await
            .map(|saved| synced(saved.id, saved.link, title, edit)),
    )
}

/// What writing a canvas's document came to: what the folder or the page's size refuses is
/// left until the canvas changes again.
fn written(answer: Result<Outcome>) -> Result<Outcome> {
    match answer {
        Err(error) if error.code == "files_folder_not_found" => {
            Ok(left(Status::Failed, "slack_canvas_folder_gone"))
        }
        Err(error) if error.code == "upload_too_large" => {
            Ok(left(Status::Failed, "upload_too_large"))
        }
        answer => answer,
    }
}

/// The picture the Slack file `id` is, as a page shows it: itself while it's small and of a
/// kind a document shows, or else Slack's largest preview of it; none for one Slack no longer
/// has, or has no picture of.
async fn picture(bot: &Bot, slack: &Slack, token: &str, id: &str) -> Result<Option<Picture>> {
    let info = match bot.call("files.info", &json!({"file": id})).await {
        Err(error) if error.code == "slack_file_gone" => return Ok(None),
        answer => answer?,
    };
    let file = &info["file"];
    let mime = file["mimetype"].as_str().unwrap_or("");
    let size = file["size"].as_u64().unwrap_or(u64::MAX);
    let size_of = |key: &str| {
        let pixels = |side: &str| file[format!("{key}_{side}")].as_u64().map(|n| n as u32);
        (pixels("w"), pixels("h"))
    };
    let chosen = if SHOWN.contains(&mime) && size <= ORIGINAL_LIMIT {
        file["url_private_download"]
            .as_str()
            .map(|url| (url, size_of("original")))
    } else {
        PREVIEWS
            .iter()
            .find_map(|key| file[*key].as_str().map(|url| (url, size_of(key))))
    };
    let Some((url, (width, height))) = chosen else {
        return Ok(None);
    };
    let bytes = read(slack.download(token, url, "image/*").await?, PICTURE_LIMIT).await?;
    Ok(kind(&bytes).map(|mime| Picture {
        mime: mime.to_owned(),
        bytes,
        width,
        height,
    }))
}

/// The kind of picture `bytes` are, by how each kind begins: one a document shows.
fn kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else {
        None
    }
}

/// What `bytes` brings, up to `limit` bytes: refused as too large past it.
async fn read(bytes: Bytestream, limit: usize) -> Result<Vec<u8>> {
    bytes
        .map_err(|_| Error::new("slack_unreachable", 502))
        .try_fold(Vec::new(), |mut read, chunk| async move {
            if read.len() + chunk.len() > limit {
                return Err(Error::new("upload_too_large", 413));
            }
            read.extend_from_slice(&chunk);
            Ok(read)
        })
        .await
}

fn synced(id: String, link: String, title: String, edit: i64) -> Outcome {
    Outcome::Synced {
        doc: id,
        link,
        title,
        edit,
    }
}
/// Waits a while, uncounted, for `reason`.
fn wait(reason: &str) -> Outcome {
    Outcome::Later {
        reason: reason.to_owned(),
        at: now() + WAIT_MS,
        counted: false,
    }
}
fn left(status: Status, reason: &str) -> Outcome {
    Outcome::Left {
        status,
        reason: reason.to_owned(),
    }
}
/// What `error` makes of trying `canvas`: it waits uncounted for an approval or the workspace
/// to work again, is tried again later each time, or is given up until it's edited again.
fn failure(canvas: &Canvas, error: &Error) -> Outcome {
    if ["slack_needs_update", "slack_connection_broken"].contains(&error.code.as_str()) {
        return wait(&error.code);
    }
    match RETRIES_MS.get(canvas.attempts.max(0) as usize) {
        Some(after) => Outcome::Later {
            reason: error.code.clone(),
            at: now() + after,
            counted: true,
        },
        None => left(Status::Failed, &error.code),
    }
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
