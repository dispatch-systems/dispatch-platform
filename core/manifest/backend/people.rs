//! The people slot: who each feature's data names, for Driver Match, which tells them apart.
//! A feature reads only its own storage; Driver Match joins what every feature names by ID.
use crate::{
    Result,
    contracts::{DriverData, DriverSource},
    db::{Db, Store},
};
use std::collections::BTreeSet;

/// One kind of a feature's data that names people, as Driver Match reads it.
pub trait People: Sync {
    /// The kind of data it is, as a person's activity lists it.
    fn data(&self) -> DriverData;
    /// The source whose IDs it names people by.
    fn source(&self) -> DriverSource;
    /// Where its spelling stands among every kind's that names the same ID: the lowest
    /// first. Amazon's sources each write a driver's name their own way.
    fn order(&self) -> u16;
    /// Every ID it names, once for each way it writes the name, in the order its names
    /// should be read.
    fn named(&self, store: &Store, dsp: &str) -> Result<Vec<Named>>;
    /// The name it writes for one ID now, if any.
    fn name(&self, store: &Store, dsp: &str, id: &str) -> Result<Option<String>>;
    /// How much it holds of each ID it names.
    fn appearances(&self, store: &Store, dsp: &str) -> Result<Vec<Appearances>>;
    /// The days it says each ID worked, and the days it was collected for.
    fn workdays(&self, _: &Store, _dsp: &str) -> Result<Workdays> {
        Ok(Workdays::default())
    }
    /// The departments whose people the DSP counts as drivers, as its feature's settings
    /// say: anyone it names outside them works in the office. `None` when they say nothing.
    fn driver_departments(&self, _: &Store, _dsp: &str) -> Result<Option<BTreeSet<String>>> {
        Ok(None)
    }
}

/// An ID as one kind of data names it.
#[derive(Clone, Debug, Default)]
pub struct Named {
    pub id: String,
    /// As written, spaces and all.
    pub name: String,
    pub department: Option<String>,
    pub position: Option<String>,
    /// The first and last days it saw them.
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    /// On its source's current roster.
    pub listed: bool,
}
impl Named {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ..Self::default()
        }
    }
    /// Extends the days it saw them by a first and last day; an empty one says nothing.
    pub fn seen(&mut self, first: &str, last: &str) {
        if !first.is_empty() && self.first_seen.as_deref().is_none_or(|f| first < f) {
            self.first_seen = Some(first.to_owned());
        }
        if !last.is_empty() && self.last_seen.as_deref().is_none_or(|l| last > l) {
            self.last_seen = Some(last.to_owned());
        }
    }
}

/// How much one kind of data holds of an ID: on how many days or weeks, and the last.
#[derive(Clone, Debug)]
pub struct Appearances {
    pub id: String,
    pub count: usize,
    pub last: String,
}

/// When one kind of data saw people at work.
#[derive(Debug, Default)]
pub struct Workdays {
    /// Each ID with a day it worked.
    pub worked: Vec<(String, String)>,
    /// The days it was collected for, so that a day outside them says nothing either way.
    pub collected: Vec<String>,
}

/// The first name `sql` finds for `id` that is more than spaces, its spaces made single.
pub fn first_name(db: &Db, sql: &str, id: &str) -> Result<Option<String>> {
    Ok(db
        .query_as::<(Option<String>,)>(sql, [id])?
        .into_iter()
        .filter_map(|(name,)| name)
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .find(|name| !name.is_empty()))
}
