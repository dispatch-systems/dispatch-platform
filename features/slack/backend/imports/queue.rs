//! Each file Slack tells of in a linked channel, kept from when it's noticed until it's saved, so
//! none is lost to a restart or a failure: a file is noticed once, by Slack's ID, and saved once.
//! One is tried as soon as it's noticed, and every minute the files due are tried again; one
//! that failed waits longer each time, and is given up after a day.
use super::{ID, LINKING, save, storage::ImportStore};
use crate::backend::{channel_links::storage::LinkStore, connection};
use dispatch_core::{
    Result, State,
    db::now,
    foundation::observability,
    manifest::{Maintenance, Upkeep, integrations::Heard},
};
use serde_json::json;
use std::{sync::Arc, time::Duration};

/// The files tried in one pass for a DSP, at most.
const BATCH: usize = 20;
/// How long a try holds its file before another may try it, in milliseconds: longer than the
/// largest file takes.
const CLAIM_MS: i64 = 30 * 60 * 1000;

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: pass,
};

/// What Import Files does with a file Slack told of: notices it, and tries it at once.
pub fn heard(state: Arc<State>, heard: Heard) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = noticed(&state, &heard).await {
            failed(&error);
        }
    })
}
async fn noticed(state: &Arc<State>, heard: &Heard) -> Result<()> {
    let event = &heard.event;
    let (Some(file), Some(channel)) = (event["file_id"].as_str(), event["channel_id"].as_str())
    else {
        return Ok(());
    };
    // Slack tells when it was shared, in seconds with a fraction.
    let posted = event["event_ts"]
        .as_str()
        .and_then(|ts| ts.parse::<f64>().ok())
        .map_or_else(now, |seconds| (seconds * 1000.0) as i64);
    let (dsp, team, file, channel) = (
        heard.dsp.clone(),
        heard.account.clone(),
        file.to_owned(),
        channel.to_owned(),
    );
    let at = dsp.clone();
    let new = state
        .run(move |db| notice(db, &at, &team, &file, &channel, posted))
        .await?;
    if new {
        save_due(state, &dsp).await;
    }
    Ok(())
}

/// Notices the file `file` posted in `channel` at `posted`, in milliseconds, if the channel is
/// linked and it was posted since, while Import Files was on: whether it's new.
pub(super) fn notice(
    db: &dispatch_core::db::Store,
    dsp: &str,
    team: &str,
    file: &str,
    channel: &str,
    posted: i64,
) -> Result<bool> {
    let Some(link) = db.slack_channel_link(dsp, &LINKING, team, channel)? else {
        return Ok(false);
    };
    let Some(on_since) = db.slack_import_on_since(dsp, team, ID)? else {
        return Ok(false);
    };
    // Slack says when a file was posted to the second: one posted the second it was linked counts.
    let since = link.linked_at.max(on_since) / 1000 * 1000;
    if posted < since {
        return Ok(false);
    }
    db.notice_slack_import(dsp, team, file, channel, now())
}

/// Every minute, the files due in each DSP Slack works for.
fn pass(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        let dsps = match state.read(|db| db.kept_dsps()).await {
            Ok(dsps) => dsps,
            Err(error) => return failed(&error),
        };
        for dsp in dsps {
            let at = dsp.clone();
            match state.read(move |db| connection::open(db, &at)).await {
                Ok(true) => save_due(&state, &dsp).await,
                Ok(false) => {}
                Err(error) => failed(&error),
            }
        }
    })
}

/// Tries the DSP's files that are due, each claimed first so two tries never run at once.
pub(super) async fn save_due(state: &Arc<State>, dsp: &str) {
    let at = dsp.to_owned();
    let due = match state
        .read(move |db| db.due_slack_imports(&at, now(), BATCH))
        .await
    {
        Ok(due) => due,
        Err(error) => return failed(&error),
    };
    for import in due {
        let (at, team, file) = (dsp.to_owned(), import.team.clone(), import.file.clone());
        let started = now();
        let claimed = state
            .run(move |db| db.claim_slack_import(&at, &team, &file, started, started + CLAIM_MS))
            .await;
        match claimed {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                failed(&error);
                continue;
            }
        }
        let outcome = save::one(state, dsp, &import).await;
        let (at, team, file) = (dsp.to_owned(), import.team, import.file);
        if let Err(error) = state
            .run(move |db| db.finish_slack_import(&at, &team, &file, &outcome, now()))
            .await
        {
            failed(&error);
        }
    }
}

fn failed(error: &dispatch_core::Error) {
    observability::event("error", "slack_import_failed", json!({"error": error.code}));
}

#[cfg(test)]
#[path = "../../tests/backend/imports.rs"]
mod tests;
