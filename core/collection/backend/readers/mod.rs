//! The live readers features declare (`manifest::live_readers`), run within the server rather
//! than as jobs: one light task for each DSP whose reader's feature is on and whose schedule is
//! set, which reads every interval from its window's start on each day the window opens, and
//! holds what its reader keeps in memory between reads. No browser starts unless a hook opens one
//! (`Reading::browser`), and at most `READS_AT_ONCE` reads run at once across the server.
//!
//! The server plans every DSP's reading as it starts, each minute, and at once when a write
//! switches a feature or saves settings (`State::readers_wake`). A task busy too long without
//! moving on is stopped, letting go of its turn to read, and started again on its own, with its
//! memory made anew, while its DSP still reads.
mod plan;
mod records;
mod task;

use crate::{
    Result, State,
    collection::{browser::ProviderBrowser, registry::Provider},
    db,
    foundation::observability,
    manifest::{Feature, Reads, registry},
};
use chrono::NaiveDate;
use chrono_tz::Tz;
use plan::Plan;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Semaphore, watch},
    task::{AbortHandle, Id, JoinSet},
};

/// How many reads run at once across the server, every reader's for every DSP: what bounds
/// the memory reading takes, and the provider's load, however many DSPs read together. A read
/// waits its turn for as long as its interval at most.
pub const READS_AT_ONCE: usize = 4;
/// How many of its intervals a DSP's task may be busy without moving on, in a read, a hook or
/// its turn to read, before it's stopped and started again. Each moves on within one interval
/// when it works.
const STALLED_AFTER: i64 = 3;
/// A task's progress while it waits for a time, as its next slot, which is no stall.
const WAITING: i64 = i64::MAX;

/// One DSP's reading, as each hook of its reader is handed it.
pub struct Reading {
    pub state: Arc<State>,
    pub dsp: String,
    /// The day its window opened on, where the DSP is: today, as the server reckons it.
    pub day: NaiveDate,
    pub timezone: Tz,
    /// The reader, by its id, whose browser closes as it stops for the DSP.
    reader: &'static str,
    /// The feature reading, whose switch the browser it opens answers to.
    feature: &'static Feature,
}
impl Reading {
    /// The DSP's browser for `provider`, signed in as a job's is, through the same limits on
    /// browsers and memory, and the reader's alone until it closes; it closes when dropped, and
    /// as the reader stops for the DSP, as when its feature is switched off.
    /// What a hook takes from it, as its session's cookies (`http::Http::signed_in`), it keeps
    /// in memory only. Refused while the feature is off for the DSP (`feature_disabled`),
    /// while another browser of the DSP's for that provider is open (`connection_busy`), and
    /// when only the owner can finish signing in (`verification_required`).
    pub async fn browser(&self, provider: Provider) -> Result<ProviderBrowser> {
        let (reader, feature) = (self.reader, self.feature.switch.id);
        ProviderBrowser::open(&self.state, &self.dsp, provider, reader, feature).await
    }
}

/// The time now, in milliseconds since the epoch, as the readers wait for it: the system's,
/// moved on as the runtime's timers are, so a wait for a slot ends when it's due by the same
/// clock. They're one clock unless the runtime's time is paused, as a test's is, which moves
/// only the timers on.
fn now() -> i64 {
    let timers = tokio::time::Instant::now().into_std();
    let system = std::time::Instant::now();
    let ahead = match timers.checked_duration_since(system) {
        Some(ahead) => ahead.as_millis() as i64,
        None => -(system.duration_since(timers).as_millis() as i64),
    };
    db::now() + ahead
}

/// A DSP's reading under way.
struct Running {
    plans: watch::Sender<Option<Plan>>,
    /// Milliseconds between its reads, by its latest plan: what its stalls are counted in.
    every: i64,
    /// Since when its task has been busy, or `WAITING`.
    progress: Arc<AtomicI64>,
    task: AbortHandle,
}
/// A reader, by its place in the registry, and a DSP.
type Key = (usize, String);

/// Runs every feature's live readers until `stop`, as the server does beside its jobs. Its
/// work waits its turn at the database, as a job's does (`State::patiently`).
pub async fn start(state: Arc<State>, stop: watch::Receiver<bool>) -> Result<()> {
    State::patiently(supervise(state, stop)).await
}

/// Plans each DSP's reading, starts and stops its tasks, and starts again those that stalled.
async fn supervise(state: Arc<State>, mut stop: watch::Receiver<bool>) -> Result<()> {
    let readers: Vec<(&'static Feature, &'static dyn Reads)> = registry().live_readers().collect();
    let reads = Arc::new(Semaphore::new(READS_AT_ONCE));
    let mut tasks = JoinSet::new();
    let mut keys: HashMap<Id, Key> = HashMap::new();
    let mut running: HashMap<Key, Running> = HashMap::new();
    let mut minute = tokio::time::interval(Duration::from_secs(60));
    minute.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = crate::cancelled(&mut stop) => break,
            _ = state.readers_wake.notified() => {}
            _ = minute.tick() => {}
            joined = tasks.join_next_with_id(), if !tasks.is_empty() => {
                let (id, failed) = match joined {
                    Some(Ok((id, ()))) => (id, false),
                    Some(Err(error)) => (error.id(), error.is_panic()),
                    None => continue,
                };
                // One the watchdog started again was forgotten as it was.
                let Some(key) = keys.remove(&id) else { continue };
                running.remove(&key);
                if failed {
                    let reader = readers[key.0].1.id();
                    observability::event(
                        "error",
                        "live.task_failed",
                        json!({"reader":reader,"dspId":key.1}),
                    );
                    continue;
                }
            }
        }
        if readers.is_empty() {
            continue;
        }
        let planned = state
            .read({
                let readers = readers.clone();
                move |db| plans(db, &readers)
            })
            .await;
        let planned = match planned {
            Ok(planned) => planned,
            Err(error) => {
                observability::event("error", "live.unplanned", json!({"error":error.code}));
                continue;
            }
        };
        let served: Vec<&Key> = planned.iter().map(|(key, _)| key).collect();
        for (key, running) in &running {
            // A DSP no longer served stops reading.
            if !served.contains(&key) && running.plans.send_replace(None).is_some() {
                state.close_reader_browsers(&key.1, readers[key.0].1.id());
            }
        }
        for (key, plan) in planned {
            let plan = match plan {
                Ok(plan) => plan,
                Err(error) => {
                    let reader = readers[key.0].1.id();
                    observability::event(
                        "error",
                        "live.unplanned",
                        json!({"reader":reader,"dspId":key.1,"error":error.code}),
                    );
                    continue;
                }
            };
            match running.get_mut(&key) {
                Some(running) => {
                    let stops = plan.is_none() && running.plans.borrow().is_some();
                    if let Some(plan) = &plan {
                        running.every = plan.every();
                    }
                    running.plans.send_if_modified(|current| {
                        let changed = *current != plan;
                        *current = plan;
                        changed
                    });
                    // What a hook still holds of the DSP's browser fails from now on.
                    if stops {
                        state.close_reader_browsers(&key.1, readers[key.0].1.id());
                    }
                }
                None => {
                    if let Some(plan) = plan {
                        let (feature, reader) = readers[key.0];
                        let task = spawn(&mut tasks, &state, feature, reader, &key.1, &reads, plan);
                        keys.insert(task.task.id(), key.clone());
                        running.insert(key, task);
                    }
                }
            }
        }
        watchdog(
            &mut running,
            &mut keys,
            &mut tasks,
            &state,
            &readers,
            &reads,
        );
    }
    tasks.shutdown().await;
    Ok(())
}

/// Each served DSP's plan for each reader: none while the reader's feature is off there or its
/// schedule isn't set, or why it couldn't be planned.
fn plans(
    db: &db::Store,
    readers: &[(&'static Feature, &'static dyn Reads)],
) -> Result<Vec<(Key, Result<Option<Plan>>)>> {
    let mut planned = Vec::new();
    for dsp in db.served_dsps()? {
        for (index, (feature, reader)) in readers.iter().enumerate() {
            let plan = (|| {
                let on = db.feature_enabled(&dsp.id, feature.switch.id)?;
                let schedule = if on {
                    reader.schedule(db, &dsp.id)?
                } else {
                    None
                };
                let timezone = dsp
                    .timezone
                    .parse()
                    .map_err(|_| crate::Error::new("invalid_timezone", 400))?;
                Ok(schedule.map(|schedule| Plan { schedule, timezone }))
            })();
            planned.push(((index, dsp.id.clone()), plan));
        }
    }
    Ok(planned)
}

fn spawn(
    tasks: &mut JoinSet<()>,
    state: &Arc<State>,
    feature: &'static Feature,
    reader: &'static dyn Reads,
    dsp: &str,
    reads: &Arc<Semaphore>,
    plan: Plan,
) -> Running {
    let (plans, receiver) = watch::channel(Some(plan));
    let progress = Arc::new(AtomicI64::new(now()));
    let task = task::Task {
        state: state.clone(),
        feature,
        reader,
        dsp: dsp.to_owned(),
        reads: reads.clone(),
        progress: progress.clone(),
    };
    let task = tasks.spawn(State::patiently(task::run(task, receiver)));
    Running {
        plans,
        every: plan.every(),
        progress,
        task,
    }
}

/// Stops each task that has been busy for `STALLED_AFTER` of its intervals, which lets go of
/// its turn to read, and starts it again, with its memory made anew, while its DSP still reads:
/// that DSP alone.
fn watchdog(
    running: &mut HashMap<Key, Running>,
    keys: &mut HashMap<Id, Key>,
    tasks: &mut JoinSet<()>,
    state: &Arc<State>,
    readers: &[(&'static Feature, &'static dyn Reads)],
    reads: &Arc<Semaphore>,
) {
    let now = now();
    for (key, stalled) in running.iter_mut() {
        let (feature, reader) = readers[key.0];
        if now - stalled.progress.load(Ordering::Acquire) <= STALLED_AFTER * stalled.every {
            continue;
        }
        observability::event(
            "warn",
            "live.stalled",
            json!({"reader":reader.id(),"dspId":key.1}),
        );
        stalled.task.abort();
        let plan = *stalled.plans.borrow();
        match plan {
            Some(plan) => {
                keys.remove(&stalled.task.id());
                *stalled = spawn(tasks, state, feature, reader, &key.1, reads, plan);
                keys.insert(stalled.task.id(), key.clone());
            }
            // One that no longer reads goes as its task ends.
            None => stalled.progress.store(WAITING, Ordering::Release),
        }
    }
}
