//! Each file Slack tells of in a linked channel, kept from when it's noticed until it's saved, so
//! none is lost to a restart or a failure: a file is noticed once, by Slack's ID, and saved once.
//! One is tried as soon as it's noticed, and every minute the files due are tried again; one
//! that failed waits longer each time, and is given up after a day.
use super::{ID, LINKING, save, storage::ImportStore};
use crate::backend::{
    channel_links::storage::LinkStore,
    work::{self, Queue},
};
use dispatch_core::{
    Result, State,
    db::{Store, now},
    manifest::{Maintenance, Upkeep, integrations::Heard},
};
use std::sync::Arc;
use std::time::Duration;

/// How a failure is reported.
const FAILED: &str = "slack_import_failed";

pub const MAINTENANCE: Maintenance = Maintenance {
    every: Duration::from_secs(60),
    run: work::pass::<Imports>,
};

/// What Import Files does with a file Slack told of: notices it, and tries it at once.
pub fn heard(state: Arc<State>, heard: Heard) -> Upkeep {
    Box::pin(async move {
        if let Err(error) = noticed(&state, &heard).await {
            work::failed(FAILED, &error);
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
    db: &Store,
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

/// Saves the DSP's files that are due.
pub(super) async fn save_due(state: &Arc<State>, dsp: &str) {
    work::due::<Imports>(state, dsp).await;
}

/// The files to save, as the pass works through them.
struct Imports;
impl Queue for Imports {
    type Item = super::storage::Import;
    type Claimed = ();
    type Outcome = super::storage::Outcome;
    const BATCH: usize = 20;
    const CLAIM_MS: i64 = 30 * 60 * 1000;
    const FAILED: &'static str = FAILED;

    fn due(db: &Store, dsp: &str, at: i64, most: usize) -> Result<Vec<Self::Item>> {
        db.due_slack_imports(dsp, at, most)
    }
    fn claim(
        db: &Store,
        dsp: &str,
        import: &Self::Item,
        at: i64,
        until: i64,
    ) -> Result<Option<()>> {
        Ok(db
            .claim_slack_import(dsp, &import.team, &import.file, at, until)?
            .then_some(()))
    }
    async fn try_one(state: &Arc<State>, dsp: &str, import: &Self::Item, (): ()) -> Self::Outcome {
        save::one(state, dsp, import).await
    }
    fn finish(
        db: &Store,
        dsp: &str,
        import: &Self::Item,
        outcome: &Self::Outcome,
        at: i64,
    ) -> Result<()> {
        db.finish_slack_import(dsp, &import.team, &import.file, outcome, at)
    }
}

#[cfg(test)]
#[path = "../../tests/backend/imports.rs"]
mod tests;
