//! Saving one file Slack told of: where it goes, what Slack says of it, who posted it, and its
//! bytes, streamed from Slack to where the DSP keeps its files as they arrive, never held whole.
//! Only a file Slack hosts is saved, up to the largest where it goes takes. What can't be done
//! yet waits: while nothing keeps the DSP's files, or what keeps them isn't ready, or the
//! workspace's admin hasn't approved reading files. A failure is tried again, later each time.
use super::{
    LINKING,
    storage::{Described, Import, ImportStore, Outcome, Status},
};
use crate::backend::{author, bot::Bot, channel_links::storage::LinkStore, slack::Slack};
use dispatch_core::{
    Error, Result, State,
    db::now,
    manifest::services::files::{self, Incoming},
    server::http::upload::Upload,
};
use serde_json::json;
use std::sync::Arc;

/// How long a file waits while it can't be saved yet, in milliseconds.
const WAIT_MS: i64 = 15 * 60 * 1000;
/// How long a file waits after each failure, in milliseconds: given up after the last.
const RETRIES_MS: &[i64] = &[
    60 * 1000,
    5 * 60 * 1000,
    15 * 60 * 1000,
    60 * 60 * 1000,
    3 * 60 * 60 * 1000,
    6 * 60 * 60 * 1000,
    12 * 60 * 60 * 1000,
];
/// Where a file came from, as where it's saved says.
const FROM: &str = "Slack";

/// Tries to save `import` for the DSP `dsp`: what it came to.
pub(super) async fn one(state: &Arc<State>, dsp: &str, import: &Import) -> Outcome {
    match attempt(state, dsp, import).await {
        Ok(outcome) => outcome,
        Err(error) => failure(import, &error),
    }
}

async fn attempt(state: &Arc<State>, dsp: &str, import: &Import) -> Result<Outcome> {
    let at = dsp.to_owned();
    let Some(files) = state.read(move |db| files::for_dsp(db, &at)).await? else {
        return Ok(wait("slack_files_unavailable"));
    };
    if let Some(why) = files.unready(state, dsp).await? {
        return Ok(wait(&why));
    }
    let (at, team, channel) = (dsp.to_owned(), import.team.clone(), import.channel.clone());
    let link = state
        .read(move |db| db.slack_channel_link(&at, &LINKING, &team, &channel))
        .await?;
    let Some(link) = link else {
        return Ok(left(Status::Skipped, "slack_import_unlinked"));
    };
    let bot = Bot::new(state, dsp, &import.team);
    let info = match bot.call("files.info", &json!({"file": import.file})).await {
        Err(error) if error.code == "slack_file_gone" => {
            return Ok(left(Status::Gone, "slack_file_gone"));
        }
        answer => answer?,
    };
    let file = &info["file"];
    if file["mode"] != "hosted" {
        return Ok(left(Status::Skipped, "slack_file_not_hosted"));
    }
    let (by, shown) = author::of(&bot, file["user"].as_str()).await?;
    let described = Described {
        name: text(&file["name"]).unwrap_or_else(|| import.file.clone()),
        mime: text(&file["mimetype"]).unwrap_or_else(|| "application/octet-stream".into()),
        size: file["size"].as_i64().unwrap_or(0),
        posted_by: text(&file["user"]),
        posted_name: Some(shown),
    };
    let (at, team, id, kept) = (
        dsp.to_owned(),
        import.team.clone(),
        import.file.clone(),
        described.clone(),
    );
    state
        .run(move |db| db.describe_slack_import(&at, &team, &id, &kept))
        .await?;
    let size = u64::try_from(described.size).unwrap_or(0);
    if size > files.largest() {
        return Ok(left(Status::TooLarge, "upload_too_large"));
    }
    let Some(url) = file["url_private_download"].as_str() else {
        return Ok(left(Status::Skipped, "slack_file_not_hosted"));
    };
    let token = bot.token().await?;
    let bytes = Slack::of(&state.config)?
        .download(&token, url, &described.mime)
        .await?;
    let incoming = Incoming {
        name: described.name,
        mime: described.mime,
        bytes: Upload::streamed(size, files.largest(), bytes).await?,
        by,
        from: FROM,
    };
    match files.save(state, dsp, &link.folder, incoming).await {
        Ok(saved) => Ok(Outcome::Saved {
            id: saved.id,
            link: saved.link,
        }),
        Err(error) if error.code == "files_folder_not_found" => {
            Ok(left(Status::Failed, "slack_import_folder_gone"))
        }
        Err(error) => Err(error),
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
/// What `error` makes of trying `import`: it waits uncounted for an approval or the workspace
/// to work again, is tried again later each time, or is given up.
fn failure(import: &Import, error: &Error) -> Outcome {
    if ["slack_needs_update", "slack_connection_broken"].contains(&error.code.as_str()) {
        return wait(&error.code);
    }
    match RETRIES_MS.get(import.attempts.max(0) as usize) {
        Some(after) => Outcome::Later {
            reason: error.code.clone(),
            at: now() + after,
            counted: true,
        },
        None => left(Status::Failed, &error.code),
    }
}

fn text(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
