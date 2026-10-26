//! The live readers slot: what a feature reads from a provider for a DSP every few minutes, on
//! the schedule each DSP sets, within the server rather than as jobs.
//!
//! A feature implements `LiveReader` and declares it in its manifest's `live_readers`. Core
//! (`collection::readers`) keeps one light task for each DSP that has the feature on while its
//! window is open: it reads every interval from the window's start, hands each whole read to
//! the feature to keep, tells it when a day starts and stops, and records each read's outcome.
use crate::{
    Result,
    collection::{browser::Pending, readers::Reading},
    db::Store,
    ensure,
    manifest::{Feature, Registry},
};
use chrono::{NaiveTime, Weekday, WeekdaySet};
use std::{any::Any, collections::BTreeSet, time::Duration};

/// What a feature reads for each DSP while its window is open, on the schedule the DSP sets.
/// Each hook is handed the DSP's `Reading`: its id, the window's day, and what it may ask of
/// core.
pub trait LiveReader: Sync + 'static {
    /// What it keeps in memory for one DSP between its reads, as a provider's session and its
    /// key: made anew for each day, and whenever the DSP's reading starts again, as after a
    /// restart, a switch or a stall; dropped as the day stops. Never written anywhere.
    type Held: Default + Send + 'static;
    /// One whole read, from `read` to `kept`.
    type Read: Send + Sync + 'static;
    /// Its id, `<feature>.<what>`: permanent, as each read's record is kept under it.
    fn id(&self) -> &'static str;
    /// The DSP's schedule, read from the feature's own settings: none while it reads nothing
    /// there. One keyed read, as the server plans every DSP's reading at once.
    fn schedule(&self, db: &Store, dsp: &str) -> Result<Option<Schedule>>;
    /// One read for the DSP: whole, or why not, by its error's code. It may open the DSP's
    /// browser for a provider (`Reading::browser`) and keep what it takes in `held`.
    fn read<'a>(
        &'a self,
        reading: &'a Reading,
        held: &'a mut Self::Held,
    ) -> Pending<'a, Self::Read>;
    /// A day's first whole read, before it is kept: what a new day starts with, as clearing
    /// the last day's. Asked again for the next whole read when it fails.
    fn started<'a>(&'a self, _reading: &'a Reading, _read: &'a Self::Read) -> Pending<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    /// Keeps a whole read, answering what it counted.
    fn kept<'a>(&'a self, reading: &'a Reading, read: Self::Read) -> Pending<'a, Counts>;
    /// A day it started, once that day's window has ended: once each, even when the server
    /// was down as it ended, before a later day starts. Asked again when it fails.
    fn stopped<'a>(&'a self, _reading: &'a Reading) -> Pending<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// What a kept read counted, each under a fixed name, as its record keeps it: never anything
/// it read.
pub type Counts = Vec<(&'static str, u64)>;

/// When a reader reads for a DSP, in the DSP's own time: on each of its days, from its start
/// until its stop, on the next day when the stop is no later than the start, all day when
/// they're the same; and while that window is open, every interval from its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    days: WeekdaySet,
    start: NaiveTime,
    stop: NaiveTime,
    every: Duration,
}
impl Schedule {
    /// From a feature's settings: the days its window opens on, its start and stop as `time`
    /// fields' values, `HH:MM`, and how often it reads. Refused unless it opens on a day at
    /// least and reads every whole number of seconds, a minute or more (`invalid_schedule`).
    pub fn new(days: &[Weekday], start: &str, stop: &str, every: Duration) -> Result<Self> {
        let time = |text: &str| {
            NaiveTime::parse_from_str(text, "%H:%M")
                .map_err(|_| crate::Error::new("invalid_schedule", 400))
        };
        let days: WeekdaySet = days.iter().copied().collect();
        ensure(
            !days.is_empty() && every >= Duration::from_secs(60) && every.subsec_nanos() == 0,
            "invalid_schedule",
            400,
        )?;
        Ok(Self {
            days,
            start: time(start)?,
            stop: time(stop)?,
            every,
        })
    }
    /// Whether its window opens on `day`.
    pub fn opens_on(&self, day: Weekday) -> bool {
        self.days.contains(day)
    }
    pub fn start(&self) -> NaiveTime {
        self.start
    }
    pub fn stop(&self) -> NaiveTime {
        self.stop
    }
    /// How often it reads while its window is open.
    pub fn every(&self) -> Duration {
        self.every
    }
}

/// A `LiveReader` as core runs it, its memory and reads unnamed: every `LiveReader` is one,
/// and a manifest lists its readers as these.
pub trait Reads: Sync {
    fn id(&self) -> &'static str;
    fn schedule(&self, db: &Store, dsp: &str) -> Result<Option<Schedule>>;
    /// A DSP's memory, as a day's reading starts.
    fn held(&self) -> Box<dyn Any + Send>;
    fn read<'a>(
        &'a self,
        reading: &'a Reading,
        held: &'a mut (dyn Any + Send),
    ) -> Pending<'a, Box<dyn Any + Send + Sync>>;
    fn started<'a>(
        &'a self,
        reading: &'a Reading,
        read: &'a (dyn Any + Send + Sync),
    ) -> Pending<'a, ()>;
    fn kept<'a>(
        &'a self,
        reading: &'a Reading,
        read: Box<dyn Any + Send + Sync>,
    ) -> Pending<'a, Counts>;
    fn stopped<'a>(&'a self, reading: &'a Reading) -> Pending<'a, ()>;
}
impl<T: LiveReader> Reads for T {
    fn id(&self) -> &'static str {
        LiveReader::id(self)
    }
    fn schedule(&self, db: &Store, dsp: &str) -> Result<Option<Schedule>> {
        LiveReader::schedule(self, db, dsp)
    }
    fn held(&self) -> Box<dyn Any + Send> {
        Box::new(T::Held::default())
    }
    fn read<'a>(
        &'a self,
        reading: &'a Reading,
        held: &'a mut (dyn Any + Send),
    ) -> Pending<'a, Box<dyn Any + Send + Sync>> {
        Box::pin(async move {
            let held = held
                .downcast_mut::<T::Held>()
                .expect("a reader is handed the memory it made");
            let read = LiveReader::read(self, reading, held).await?;
            Ok(Box::new(read) as Box<dyn Any + Send + Sync>)
        })
    }
    fn started<'a>(
        &'a self,
        reading: &'a Reading,
        read: &'a (dyn Any + Send + Sync),
    ) -> Pending<'a, ()> {
        let read = read
            .downcast_ref::<T::Read>()
            .expect("a reader is handed the read it made");
        LiveReader::started(self, reading, read)
    }
    fn kept<'a>(
        &'a self,
        reading: &'a Reading,
        read: Box<dyn Any + Send + Sync>,
    ) -> Pending<'a, Counts> {
        let read = read
            .downcast::<T::Read>()
            .expect("a reader is handed the read it made");
        LiveReader::kept(self, reading, *read)
    }
    fn stopped<'a>(&'a self, reading: &'a Reading) -> Pending<'a, ()> {
        LiveReader::stopped(self, reading)
    }
}

impl Registry {
    /// Every feature's live readers, each with its feature, in the registry's order.
    pub fn live_readers(&self) -> impl Iterator<Item = (&'static Feature, &'static dyn Reads)> {
        self.features.iter().flat_map(|feature| {
            feature
                .live_readers
                .iter()
                .map(move |each| (*feature, *each))
        })
    }
}

/// Panics unless each live reader has an id of its own, under its feature's name.
pub(crate) fn check(registry: &Registry) {
    let mut seen = BTreeSet::new();
    for (feature, each) in registry.live_readers() {
        assert!(
            each.id().starts_with(&format!("{}.", feature.name)),
            "{} declares the live reader {}, which is not under its name",
            feature.name,
            each.id()
        );
        assert!(
            seen.insert(each.id()),
            "the live reader {} is declared twice",
            each.id()
        );
    }
}
