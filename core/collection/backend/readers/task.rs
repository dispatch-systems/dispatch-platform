//! One DSP's reading for one reader: a light task that waits for each slot of its window,
//! reads, hands what it read to its reader to keep, records how it went, and stops each day it
//! started once its window ends.
use super::{
    Reading, now,
    plan::{Day, Plan, Step},
    records,
};
use crate::{
    Error, State,
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
use tokio::sync::{Semaphore, watch};

/// What a task is handed: its reader, the DSP, the server's turns at reading, and where it
/// says it moved on, for the watchdog.
pub(super) struct Task {
    pub state: Arc<State>,
    pub feature: &'static Feature,
    pub reader: &'static dyn Reads,
    pub dsp: String,
    pub reads: Arc<Semaphore>,
    pub progress: Arc<AtomicI64>,
}

/// Reads for the DSP as its plan says, until the plan is none, when what's under way ends: a
/// read is dropped, while what keeps it or a day finishes.
pub(super) async fn run(task: Task, mut plans: watch::Receiver<Option<Plan>>) {
    // Its next slot is the first after its last read, whenever that was.
    let mut after = loop {
        if plans.borrow().is_none() {
            return;
        }
        match task
            .query(|db, reader, dsp| db.last_read(reader, dsp))
            .await
        {
            Some(last) => break last.unwrap_or(i64::MIN),
            None => task.retry(&mut plans).await,
        }
    };
    // What the reader holds for the day being read.
    let mut held: Option<(NaiveDate, Box<dyn Any + Send>)> = None;
    loop {
        task.moved();
        let Some(plan) = *plans.borrow_and_update() else {
            return;
        };
        let Some(unstopped) = task
            .query(|db, reader, dsp| db.unstopped_days(reader, dsp))
            .await
        else {
            task.retry(&mut plans).await;
            continue;
        };
        let at = now();
        match plan.step(at, after, &unstopped) {
            Step::Wait(at) => {
                let wait = Duration::from_millis((at - now()).max(0) as u64);
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
                if held.as_ref().is_none_or(|(date, _)| *date != day.date) {
                    held = Some((day.date, task.reader.held()));
                }
                let memory = &mut held.as_mut().expect("held for the day").1;
                if task
                    .read_once(&plan, day, memory.as_mut(), &mut plans)
                    .await
                {
                    // Its next slot is after this one, though the clock reads a moment
                    // earlier than it did.
                    after = now().max(at);
                } else {
                    return;
                }
            }
        }
    }
}

impl Task {
    fn moved(&self) {
        self.progress.store(now(), Ordering::Release);
    }
    fn reading(&self, plan: &Plan, day: &Day) -> Reading {
        Reading {
            state: self.state.clone(),
            dsp: self.dsp.clone(),
            day: clock::day(plan.timezone, day.open),
            timezone: plan.timezone,
            feature: self.feature,
        }
    }
    fn log(&self, level: &str, event: &str, fields: serde_json::Value) {
        let mut fields = fields;
        fields["reader"] = json!(self.reader.id());
        fields["dspId"] = json!(self.dsp);
        observability::event(level, event, fields);
    }

    /// One read of `day`, waiting its turn among the server's reads, kept when whole, and
    /// recorded. Answers false once its plan is none.
    async fn read_once(
        &self,
        plan: &Plan,
        day: Day,
        held: &mut (dyn Any + Send),
        plans: &mut watch::Receiver<Option<Plan>>,
    ) -> bool {
        let every = Duration::from_millis(plan.every as u64);
        let reading = self.reading(plan, &day);
        let waited = now();
        // A read that can't start within its interval missed its slot, as when the server
        // reads too much at once.
        let turn = tokio::select! {
            turn = tokio::time::timeout(every, self.reads.clone().acquire_owned()) => turn,
            _ = plans.wait_for(Option::is_none) => return false,
        };
        let Ok(Ok(_turn)) = turn else {
            self.record(&day, waited, Err(Error::new("live_reads_busy", 503)))
                .await;
            return true;
        };
        self.moved();
        let at = now();
        let read = tokio::select! {
            read = tokio::time::timeout(every, self.reader.read(&reading, held)) => {
                read.unwrap_or_else(|_| Err(Error::new("live_read_timeout", 504)))
            }
            _ = plans.wait_for(Option::is_none) => return false,
        };
        self.moved();
        let outcome = match read {
            Ok(read) => self.keep(&reading, &day, read).await,
            Err(error) => Err(error),
        };
        self.record(&day, at, outcome).await;
        true
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

    /// Stops `date`, answering whether it did.
    async fn stop(&self, plan: &Plan, date: NaiveDate) -> bool {
        let day = plan.day(date);
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
        self.moved();
        stopped.is_ok()
    }

    async fn record(&self, day: &Day, at: i64, outcome: crate::Result<crate::manifest::Counts>) {
        let outcome = outcome.map_err(|error| error.code);
        if let Err(code) = &outcome {
            self.log(
                "warn",
                "live.read_failed",
                json!({"day":day.date.to_string(),"error":code}),
            );
        }
        let (date, ms) = (day.date, (now() - at).max(0));
        let kept = self
            .write(move |db, reader, dsp| {
                db.record_read(&records::Read {
                    reader,
                    dsp,
                    day: date,
                    at,
                    ms,
                    outcome,
                })
            })
            .await;
        if let Err(error) = kept {
            self.log("error", "live.record_failed", json!({"error":error.code}));
        }
        self.moved();
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
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            _ = plans.changed() => {}
        }
    }
}
