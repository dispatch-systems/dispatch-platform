//! One DSP's reading for one reader: a light task that waits for each slot of its window,
//! reads, hands what it read to its reader to keep, records how it went, and stops each day it
//! started once its window ends. A read the browsers' memory turns away is tried once more
//! within its slot. Its next slot is the first after its last read ended, as that read's record
//! keeps it, so started again it goes on as it would have.
use super::{
    Reading, WAITING, now,
    plan::{Day, Plan, Step},
    records,
};
use crate::{
    Code, Error, State,
    db::Store,
    foundation::{clock, observability},
    manifest::{Feature, Reads},
};
use chrono::NaiveDate;
use serde_json::json;
use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    time::Instant,
};

/// How long a read the browsers' memory turned away waits before it's tried again.
const MEMORY_BUSY_RETRY: Duration = Duration::from_secs(60);

/// What a task is handed: its reader, the DSP, the server's turns at reading, and since when
/// it has been busy, for the watchdog.
pub(super) struct Task {
    pub state: Arc<State>,
    pub feature: &'static Feature,
    pub reader: &'static dyn Reads,
    pub dsp: String,
    pub reads: Arc<Semaphore>,
    pub progress: Arc<AtomicI64>,
}

/// Reads for the DSP as its plan says, until the plan is none, when what's under way ends: a
/// read is dropped and recorded so, while what keeps it or a day finishes.
pub(super) async fn run(task: Task, mut plans: watch::Receiver<Option<Plan>>) {
    // Its next slot is the first after its last read ended, whenever that was.
    let mut after = loop {
        if plans.borrow().is_none() {
            return;
        }
        match task
            .query(|db, reader, dsp| Ok(db.last_read(reader, dsp)?.map(|read| read.ended)))
            .await
        {
            Some(last) => break last.unwrap_or(i64::MIN),
            None => task.retry(&mut plans).await,
        }
    };
    // What the reader holds for the day being read.
    let mut held: Option<(NaiveDate, Box<dyn Any + Send>)> = None;
    loop {
        task.busy();
        let Some(plan) = *plans.borrow_and_update() else {
            return;
        };
        let Some((unstopped, ended)) = task
            .query(|db, reader, dsp| {
                Ok((db.unstopped_days(reader, dsp)?, db.last_ended(reader, dsp)?))
            })
            .await
        else {
            task.retry(&mut plans).await;
            continue;
        };
        let at = now();
        match plan.step(at, after, &unstopped, ended) {
            Step::Wait(at) => {
                let wait = Duration::from_millis((at - now()).max(0) as u64);
                task.waiting();
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    changed = plans.changed() => if changed.is_err() { return },
                }
            }
            Step::Stop(date) => {
                if held.as_ref().is_some_and(|(day, _)| *day == date) {
                    held = None;
                }
                if !task.stop(&plan, date).await {
                    task.retry(&mut plans).await;
                }
            }
            Step::Read(day) => {
                // A day's first read makes its memory, and lets the records from before the
                // last two weeks go.
                if held.as_ref().is_none_or(|(date, _)| *date != day.date) {
                    held = Some((day.date, task.reader.held()));
                    if let Err(error) = task
                        .write(move |db, reader, dsp| db.prune_reads(reader, dsp, at))
                        .await
                    {
                        task.log("error", "live.prune_failed", json!({"error":error.code}));
                    }
                }
                let memory = &mut held.as_mut().expect("held for the day").1;
                match task
                    .read_once(&plan, day, at, memory.as_mut(), &mut plans)
                    .await
                {
                    Slot::Recorded(ended) => after = ended,
                    Slot::Unrecorded => {}
                    Slot::Ended => return,
                }
            }
        }
    }
}

impl Task {
    /// Busy from now: in a read, a hook or its turn to read.
    fn busy(&self) {
        self.progress.store(now(), Ordering::Release);
    }
    /// Waiting for a time, as its next slot, which is no stall.
    fn waiting(&self) {
        self.progress.store(WAITING, Ordering::Release);
    }
    fn reading(&self, plan: &Plan, day: &Day) -> Reading {
        Reading {
            state: self.state.clone(),
            dsp: self.dsp.clone(),
            day: clock::day(plan.timezone, day.open),
            timezone: plan.timezone,
            reader: self.reader.id(),
            feature: self.feature,
        }
    }
    fn log(&self, level: &str, event: &str, fields: serde_json::Value) {
        let mut fields = fields;
        fields["reader"] = json!(self.reader.id());
        fields["dspId"] = json!(self.dsp);
        observability::event(level, event, fields);
    }

    /// The read of `day`'s slot its task came to at `since`, waiting its turn among the
    /// server's reads, kept when whole, and recorded as beginning and ending no earlier than
    /// `since`: where it leaves the task. A read whose turn comes once its window has closed or
    /// its plan has changed is left to plan again, unrecorded. One the browsers' memory turned
    /// away is tried once more a minute on, its turn let go meanwhile, while that's before its
    /// next slot and its window's close: the slot keeps the record of the retry once it starts,
    /// and of the first try otherwise.
    async fn read_once(
        &self,
        plan: &Plan,
        day: Day,
        since: i64,
        held: &mut (dyn Any + Send),
        plans: &mut watch::Receiver<Option<Plan>>,
    ) -> Slot {
        let reading = self.reading(plan, &day);
        let mut attempt = self.attempt(plan, &reading, held, plans, day.close).await;
        // The first try, logged as it failed, while a retry may replace its record.
        let mut first = None;
        if let Attempt::Failed(at, ended, error) = &attempt
            && let Some(until) = retry_before(plan, &day, (*ended).max(since), error)
        {
            self.failed(&day, &error.code);
            first = Some((*at, *ended, error.code.clone()));
            self.waiting();
            let planned = tokio::select! {
                _ = tokio::time::sleep(MEMORY_BUSY_RETRY) => true,
                _ = plans.wait_for(Option::is_none) => false,
            };
            attempt = if planned {
                self.busy();
                self.attempt(plan, &reading, held, plans, until).await
            } else {
                Attempt::Dropped(None)
            };
        }
        let ends = matches!(attempt, Attempt::Dropped(_));
        let ended = match attempt {
            Attempt::Read(_turn, at, read) => {
                let outcome = self.keep(&reading, &day, read).await;
                self.record(&day, since, at, now(), outcome).await
            }
            Attempt::Failed(at, ended, error) => {
                self.record(&day, since, at, ended, Err(error)).await
            }
            Attempt::Busy(waited) => {
                let busy = Err(Error::new("live_reads_busy", 503));
                self.record(&day, since, waited, now(), busy).await
            }
            Attempt::Dropped(Some(at)) => {
                let stopped = Err(Error::new("live_read_stopped", 409));
                self.record(&day, since, at, now(), stopped).await
            }
            // A retry that never started leaves the first try on record.
            Attempt::Late | Attempt::Dropped(None) => match first {
                Some((at, ended, code)) => {
                    self.write_record(&day, since, at, ended, Err(code)).await
                }
                None if ends => return Slot::Ended,
                None => return Slot::Unrecorded,
            },
        };
        if ends {
            Slot::Ended
        } else {
            Slot::Recorded(ended)
        }
    }

    /// One try at a read, which must start before `before`: its turn among the server's reads,
    /// waited for for the plan's timeout at most, then the read, for that timeout at most.
    async fn attempt(
        &self,
        plan: &Plan,
        reading: &Reading,
        held: &mut (dyn Any + Send),
        plans: &mut watch::Receiver<Option<Plan>>,
        before: i64,
    ) -> Attempt {
        let waited = now();
        // `before`, as the runtime's timers keep time: a turn that comes then, or none by then,
        // is late, though the wall clock reads a moment earlier.
        let start = Instant::now();
        let late = start + Duration::from_millis((before - waited).max(0) as u64);
        // A read that can't start within its timeout missed its slot, as when the server reads
        // too much at once.
        let wait = late.min(start + plan.timeout());
        let turn = tokio::select! {
            turn = tokio::time::timeout_at(wait, self.reads.clone().acquire_owned()) => turn,
            _ = plans.wait_for(Option::is_none) => return Attempt::Dropped(None),
        };
        if Instant::now() >= late {
            return Attempt::Late;
        }
        let Ok(Ok(turn)) = turn else {
            return Attempt::Busy(waited);
        };
        self.busy();
        let at = now();
        if *plans.borrow() != Some(*plan) {
            return Attempt::Late;
        }
        let read = tokio::select! {
            read = tokio::time::timeout(plan.timeout(), self.reader.read(reading, held)) => {
                read.unwrap_or_else(|_| Err(Error::new("live_read_timeout", 504)))
            }
            _ = plans.wait_for(Option::is_none) => return Attempt::Dropped(Some(at)),
        };
        self.busy();
        match read {
            Ok(read) => Attempt::Read(turn, at, read),
            Err(error) => Attempt::Failed(at, now(), error),
        }
    }

    /// Keeps a whole read: a day's first starts the day first.
    async fn keep(
        &self,
        reading: &Reading,
        day: &Day,
        read: Box<dyn Any + Send + Sync>,
    ) -> crate::Result<crate::manifest::Counts> {
        let date = day.date;
        let started = self
            .query(move |db, reader, dsp| db.day_started(reader, dsp, date))
            .await
            .ok_or_else(|| Error::new("live_days_unavailable", 503))?;
        if !started {
            self.reader.started(reading, read.as_ref()).await?;
            self.log("info", "live.started", json!({"day":date.to_string()}));
        }
        let at = now();
        self.write(move |db, reader, dsp| db.keep_day(reader, dsp, date, at))
            .await?;
        self.reader.kept(reading, read).await
    }

    /// Stops `date`, answering whether it did: its stop begun first, so the day reads no more
    /// though its hook fails part of the way, as after it emptied what the day kept. Its hook
    /// waits its turn among the server's reads for as long as that takes, as every DSP stopping
    /// at the same time saves its day at once.
    async fn stop(&self, plan: &Plan, date: NaiveDate) -> bool {
        let day = plan.day(date);
        let began = now();
        if let Err(error) = self
            .write(move |db, reader, dsp| db.begin_stop(reader, dsp, date, began))
            .await
        {
            self.log(
                "warn",
                "live.stop_failed",
                json!({"day":date.to_string(),"error":error.code}),
            );
            return false;
        }
        self.waiting();
        let Ok(_turn) = self.reads.acquire().await else {
            return false;
        };
        self.busy();
        let stopped = self.reader.stopped(&self.reading(plan, &day)).await;
        let at = now();
        let stopped = match stopped {
            Ok(()) => {
                self.write(move |db, reader, dsp| db.stop_day(reader, dsp, date, at))
                    .await
            }
            Err(error) => Err(error),
        };
        match &stopped {
            Ok(()) => self.log("info", "live.stopped", json!({"day":date.to_string()})),
            Err(error) => self.log(
                "warn",
                "live.stop_failed",
                json!({"day":date.to_string(),"error":error.code}),
            ),
        }
        self.busy();
        stopped.is_ok()
    }

    /// Records a read of `day` that began at `at` and ended at `ended`, its slot come to at
    /// `since`, logging a failure: answers when it ended, as its record keeps it.
    async fn record(
        &self,
        day: &Day,
        since: i64,
        at: i64,
        ended: i64,
        outcome: crate::Result<crate::manifest::Counts>,
    ) -> i64 {
        let outcome = outcome.map_err(|error| error.code);
        if let Err(code) = &outcome {
            self.failed(day, code);
        }
        self.write_record(day, since, at, ended, outcome).await
    }
    /// Writes a read's record, beginning and ending no earlier than `since`, so its task's next
    /// slot is past the one it came to though the clock read a moment earlier then: answers
    /// when it ended, as the record keeps it.
    async fn write_record(
        &self,
        day: &Day,
        since: i64,
        at: i64,
        ended: i64,
        outcome: std::result::Result<crate::manifest::Counts, String>,
    ) -> i64 {
        let at = at.max(since);
        let ended = ended.max(at);
        let date = day.date;
        let kept = self
            .write(move |db, reader, dsp| {
                db.record_read(&records::Read {
                    reader,
                    dsp,
                    day: date,
                    at,
                    ms: ended - at,
                    outcome,
                })
            })
            .await;
        if let Err(error) = kept {
            self.log("error", "live.record_failed", json!({"error":error.code}));
        }
        self.busy();
        ended
    }

    /// Notes in the server's log a read of `day` that failed, by its error's code.
    fn failed(&self, day: &Day, code: &str) {
        self.log(
            "warn",
            "live.read_failed",
            json!({"day":day.date.to_string(),"error":code}),
        );
    }

    /// What `f` reads of the DSP's reading, or none when it can't be read now, logged.
    async fn query<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store, &str, &str) -> crate::Result<T> + Send + 'static,
    ) -> Option<T> {
        let (reader, dsp) = (self.reader.id(), self.dsp.clone());
        match self.state.read(move |db| f(db, reader, &dsp)).await {
            Ok(value) => Some(value),
            Err(error) => {
                self.log("error", "live.unreadable", json!({"error":error.code}));
                None
            }
        }
    }
    async fn write(
        &self,
        f: impl FnOnce(&Store, &str, &str) -> crate::Result<()> + Send + 'static,
    ) -> crate::Result<()> {
        let (reader, dsp) = (self.reader.id(), self.dsp.clone());
        self.state
            .run_bookkeeping(move |db| f(db, reader, &dsp))
            .await
    }
    /// Waits a minute, or until its plan changes, before trying again what failed.
    async fn retry(&self, plans: &mut watch::Receiver<Option<Plan>>) {
        self.waiting();
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            _ = plans.changed() => {}
        }
    }
}

/// What came of one try at a read.
enum Attempt {
    /// It read whole, from the instant it holds, its turn held until what it read is kept.
    Read(OwnedSemaphorePermit, i64, Box<dyn Any + Send + Sync>),
    /// It read from the first instant it holds until the second, and failed: it holds no turn,
    /// as there's nothing to keep.
    Failed(i64, i64, Error),
    /// Its turn didn't come within its timeout, begun at the instant it holds.
    Busy(i64),
    /// Its turn came no earlier than it had to start before, or none came before then, or its
    /// plan changed.
    Late,
    /// Its plan became none: as it read, from the instant it holds, or as it waited for its
    /// turn.
    Dropped(Option<i64>),
}

/// Where a slot's read leaves its task.
enum Slot {
    /// On record, ended at the instant it holds: its next slot is the first after it.
    Recorded(i64),
    /// With no record: its slot reads as planned again.
    Unrecorded,
    /// Its plan is none: the task ends.
    Ended,
}

/// Before when a read of `day` the browsers' memory turned away may be tried again, its record
/// ending at `ended`: its next slot, or its window's close when that's sooner, while a retry a
/// minute on comes before it.
fn retry_before(plan: &Plan, day: &Day, ended: i64, error: &Error) -> Option<i64> {
    let until = plan.slot_after(day, ended).min(day.close);
    let retry = now() + MEMORY_BUSY_RETRY.as_millis() as i64;
    (error.is(Code::BrowserMemoryBusy) && retry < until).then_some(until)
}
