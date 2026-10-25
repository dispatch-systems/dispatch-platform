//! The live readers slot: what a feature reads from a provider for a DSP every few minutes,
//! inside a window each DSP sets, within the server rather than as jobs.
//!
//! A feature implements `LiveReader` and declares it in its manifest's `live_readers`. Core
//! (`collection::readers`) keeps one light task for each DSP that has the feature on while its
//! window is open: it reads on a grid from the window's start, hands each whole read to the
//! feature to keep, tells it when a day starts and stops, and records each read's outcome.
use crate::{
    Result,
    collection::{browser::Pending, readers::Reading},
    db::Store,
    manifest::{Feature, Registry},
};
use chrono::NaiveTime;
use std::{any::Any, collections::BTreeSet, time::Duration};

/// What a feature reads for each DSP, every `every`, while the DSP's window is open. Each hook
/// is handed the DSP's `Reading`: its id, the window's day, and what it may ask of core.
pub trait LiveReader: Sync + 'static {
    /// What it keeps in memory for one DSP between its reads, as a provider's session and its
    /// key: made anew for each day, and whenever the DSP's reading starts again, as after a
    /// restart, a switch or a stall; dropped as the day stops. Never written anywhere.
    type Held: Default + Send + 'static;
    /// One whole read, from `read` to `kept`.
    type Read: Send + Sync + 'static;
    /// Its id, `<feature>.<what>`: permanent, as each read's record is kept under it.
    fn id(&self) -> &'static str;
    /// How often it reads while a window is open: whole seconds, a minute at least.
    fn every(&self) -> Duration;
    /// The DSP's window, read from the feature's own settings: none while it reads nothing
    /// there. One keyed read, as the server plans every DSP's reading at once.
    fn window(&self, db: &Store, dsp: &str) -> Result<Option<Window>>;
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

/// When a reader reads for a DSP each day, in the DSP's own time: from `start` until `stop`,
/// on the next day when `stop` is no later than `start`, all day when they're the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub start: NaiveTime,
    pub stop: NaiveTime,
}
impl Window {
    /// From two `time` fields' values, `HH:MM`, as a feature's settings read them.
    pub fn parse(start: &str, stop: &str) -> Result<Self> {
        let time = |text: &str| {
            NaiveTime::parse_from_str(text, "%H:%M")
                .map_err(|_| crate::Error::new("invalid_window", 400))
        };
        Ok(Self {
            start: time(start)?,
            stop: time(stop)?,
        })
    }
}

/// A `LiveReader` as core runs it, its memory and reads unnamed: every `LiveReader` is one,
/// and a manifest lists its readers as these.
pub trait Reads: Sync {
    fn id(&self) -> &'static str;
    fn every(&self) -> Duration;
    fn window(&self, db: &Store, dsp: &str) -> Result<Option<Window>>;
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
    fn every(&self) -> Duration {
        LiveReader::every(self)
    }
    fn window(&self, db: &Store, dsp: &str) -> Result<Option<Window>> {
        LiveReader::window(self, db, dsp)
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

/// Panics unless each live reader has an id of its own, under its feature's name, and reads
/// every whole number of seconds, a minute or more.
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
        let every = each.every();
        assert!(
            every >= Duration::from_secs(60) && every.subsec_nanos() == 0,
            "the live reader {} reads every {every:?}: whole seconds, a minute at least",
            each.id()
        );
    }
}
