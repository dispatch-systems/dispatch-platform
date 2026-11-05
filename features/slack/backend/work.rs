//! What Slack's passes share. Each of its own integrations keeps work until it's done, as
//! Import Files keeps each file to save and Sync Canvases each canvas to sync: noticed in the
//! DSP's database, tried, and tried again later when a try fails, so none is lost to a restart
//! or a failure. A pass goes over each DSP Slack works for; each item due is claimed first, so
//! two tries never run at once, and what its try came to is kept. Each pass logs its own
//! failures and goes on to the next DSP.
use super::{connection, storage::SlackStore};
use dispatch_core::{
    Error, Result, State,
    db::{Store, now},
    foundation::observability,
    manifest::Upkeep,
    server::cache::DataDomain,
};
use serde_json::json;
use std::{future::Future, sync::Arc, time::Duration};

/// How long work Slack told of waits before each try again after it fails: a second, five and
/// thirty.
const AGAIN: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
];

/// Reports a pass's failure, as `event`, with the error's code and the DSP it was working for, if
/// it was working for one.
pub fn failed(event: &'static str, dsp: Option<&str>, error: &Error) {
    observability::event("error", event, json!({"dspId": dsp, "error": error.code}));
}

/// Does work Slack told of, `work`, until it succeeds: tried again after a second, five and thirty
/// when it fails, then left to the passes that catch up on what was missed, answering why its last
/// try failed. Each try does it all again, told whether it's again, so what it keeps it keeps
/// once, however often it's tried; and what came after it in Slack is kept meanwhile, so a try
/// again keeps nothing older than what's kept.
pub async fn retried<F: Future<Output = Result<()>>>(
    mut work: impl FnMut(bool) -> F,
) -> Result<()> {
    for (tries, wait) in AGAIN.into_iter().enumerate() {
        if work(tries > 0).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(wait).await;
    }
    work(true).await
}

/// The DSPs Slack works for: each DSP served with Slack on. A failure finding them is reported
/// as `event`, and the pass finds none.
pub async fn dsps(state: &Arc<State>, event: &'static str) -> Vec<String> {
    let found = state
        .read(|db| {
            let mut open = Vec::new();
            for dsp in db.served_ids()? {
                if connection::open(db, &dsp)? {
                    open.push(dsp);
                }
            }
            Ok(open)
        })
        .await;
    match found {
        Ok(open) => open,
        Err(error) => {
            failed(event, None, &error);
            Vec::new()
        }
    }
}

/// The DSP's workspaces the integration `integration` works in: each working workspace it's
/// switched on in, by its team ID.
pub(crate) fn workspaces_on(db: &Store, dsp: &str, integration: &str) -> Result<Vec<String>> {
    let mut on = Vec::new();
    for workspace in db.slack_workspaces(dsp)? {
        if !workspace.broken
            && db
                .slack_integrations(dsp, &workspace.team)?
                .iter()
                .any(|id| id == integration)
        {
            on.push(workspace.team);
        }
    }
    Ok(on)
}

/// Work an integration keeps until it's done: how what's due is read, claimed, tried and
/// finished, each in the DSP's database but the try.
pub trait Queue: 'static {
    /// An item of it, as kept.
    type Item: Clone + Send + 'static;
    /// What claiming an item finds for its try.
    type Claimed: Send + 'static;
    /// What a try came to.
    type Outcome: Send + 'static;
    /// How many items are tried in one pass for a DSP, at most.
    const BATCH: usize;
    /// How long a try holds its item before another may try it, in milliseconds: longer than
    /// the largest item takes.
    const CLAIM_MS: i64;
    /// How a failure is reported.
    const FAILED: &'static str;
    /// The kind of the DSP's data its claims and what its tries came to change, for the read
    /// cache: its owner's, evicting the cached reads that depend on it; none while no cached read
    /// depends on what it keeps, as bookkeeping.
    const CHANGES: Option<DataDomain>;

    /// The items waiting to be tried by `at`, the longest waiting first, `most` at most.
    fn due(db: &Store, dsp: &str, at: i64, most: usize) -> Result<Vec<Self::Item>>;
    /// Claims the item for one try until `until`, with what its try needs: none when it was
    /// no longer due at `at`, as another try has it.
    fn claim(
        db: &Store,
        dsp: &str,
        item: &Self::Item,
        at: i64,
        until: i64,
    ) -> Result<Option<Self::Claimed>>;
    /// One try, which answers what it came to, a failure included.
    fn try_one(
        state: &Arc<State>,
        dsp: &str,
        item: &Self::Item,
        claimed: Self::Claimed,
    ) -> impl Future<Output = Self::Outcome> + Send;
    /// Keeps what the try came to, at `at`.
    fn finish(
        db: &Store,
        dsp: &str,
        item: &Self::Item,
        outcome: &Self::Outcome,
        at: i64,
    ) -> Result<()>;
}

/// The pass that tries what's due in each DSP Slack works for: a `Maintenance` task's `run`.
pub fn pass<Q: Queue>(state: Arc<State>, _: bool) -> Upkeep {
    Box::pin(async move {
        for dsp in dsps(&state, Q::FAILED).await {
            due::<Q>(&state, &dsp).await;
        }
    })
}

/// Tries the DSP's items that are due while Slack works for it, each claimed first so two tries
/// never run at once: none once the DSP is disabled, or Slack switched off there, even in a
/// pass under way. The try under way then finds the bot refusing to act (`Bot`), and waits,
/// uncounted, as its integration says.
pub async fn due<Q: Queue>(state: &Arc<State>, dsp: &str) {
    let at = dsp.to_owned();
    let due = state
        .read(move |db| {
            if connection::open(db, &at)? {
                Q::due(db, &at, now(), Q::BATCH)
            } else {
                Ok(Vec::new())
            }
        })
        .await;
    let due = match due {
        Ok(due) => due,
        Err(error) => return failed(Q::FAILED, Some(dsp), &error),
    };
    for item in due {
        let (at, of) = (dsp.to_owned(), item.clone());
        let started = now();
        let claimed = write::<Q, _>(state, dsp, move |db| {
            if !connection::open(db, &at)? {
                return Ok(None);
            }
            Q::claim(db, &at, &of, started, started + Q::CLAIM_MS)
        })
        .await;
        let outcome = match claimed {
            Ok(Some(claimed)) => Q::try_one(state, dsp, &item, claimed).await,
            Ok(None) => continue,
            Err(error) => {
                failed(Q::FAILED, Some(dsp), &error);
                continue;
            }
        };
        let at = dsp.to_owned();
        if let Err(error) = write::<Q, _>(state, dsp, move |db| {
            Q::finish(db, &at, &item, &outcome, now())
        })
        .await
        {
            failed(Q::FAILED, Some(dsp), &error);
        }
    }
}
/// A write of the queue `Q` in the DSP's database, evicting only the cached reads that depend on
/// what it changes (`Queue::CHANGES`).
async fn write<Q: Queue, T: Send + 'static>(
    state: &Arc<State>,
    dsp: &str,
    f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
) -> Result<T> {
    match Q::CHANGES {
        Some(domain) => state.run_scoped(dsp, domain, f).await,
        None => state.run_bookkeeping(f).await,
    }
}

#[cfg(test)]
#[path = "../tests/backend/work.rs"]
mod tests;
