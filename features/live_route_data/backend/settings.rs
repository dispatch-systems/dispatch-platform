//! Live Route Data's settings, on its own page of a DSP's Settings, which draws them from this
//! declaration alone.
use chrono::{Weekday, WeekdaySet};
use dispatch_core::{
    Error, Result,
    db::Store,
    ensure,
    manifest::{Schedule, Section, choice, days, section, time, toggle},
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// When Live Route Data reads a DSP's routes, where the DSP is: on which days, from when until
/// when, and how often; and whether the day is saved into historical route data as it stops, for
/// those holding `live_route_data.manage`. Its id and its fields' are permanent once shipped:
/// each DSP's values are kept under them.
pub const COLLECTION: Section = section(
    "live_route_data.collection",
    "Collection",
    "live_route_data.manage",
    &[
        days("days", "Days", WeekdaySet::ALL),
        time("start", "Start", "08:00"),
        time("stop", "Stop", "21:00"),
        choice("every", "Every", EVERY, "5"),
        toggle("save_to_history", "Save to historical route data", true),
    ],
)
.saved(checked);

/// How often it may read, in minutes, each with its label.
const EVERY: &[(&str, &str)] = &[
    ("5", "5 min"),
    ("10", "10 min"),
    ("15", "15 min"),
    ("30", "30 min"),
    ("60", "1 hr"),
];

/// `COLLECTION`'s values, each field by its id: its default until the DSP saves one.
#[derive(Deserialize)]
pub struct Collection {
    pub days: Vec<Weekday>,
    pub start: String,
    pub stop: String,
    /// How often it reads, in minutes: one of `EVERY`'s.
    pub every: String,
    /// Whether the day is saved into historical route data as it stops.
    pub save_to_history: bool,
}
impl Collection {
    /// When the DSP's routes are read, as the live readers keep it.
    pub fn schedule(&self) -> Result<Schedule> {
        let minutes: u64 = (self.every.parse()).map_err(|_| Error::new("invalid_input", 400))?;
        let every = Duration::from_secs(minutes * 60);
        Schedule::new(&self.days, &self.start, &self.stop, every)
    }
}

/// The DSP's `COLLECTION`, as Live Route Data reads it: one keyed lookup, of the DSP the caller
/// was authorized for.
pub(crate) fn collection(db: &Store, dsp: &str) -> Result<Collection> {
    dispatch_core::settings::read(db, dsp, &COLLECTION)
}

/// Refuses, in the save's own write, a schedule on no day (`live_route_data_days_required`),
/// and a window whose stop is no later than its start (`live_route_data_window_invalid`): a live
/// day is one calendar day where the DSP is, so its window never runs past midnight, and every
/// read of it is a read of today.
fn checked(_: &Store, _: &str, values: &Value) -> Result<()> {
    let collection = Collection::deserialize(values)?;
    ensure(
        !collection.days.is_empty(),
        "live_route_data_days_required",
        400,
    )?;
    let schedule = collection.schedule()?;
    ensure(
        schedule.stop() > schedule.start(),
        "live_route_data_window_invalid",
        400,
    )
}

#[cfg(test)]
#[path = "../tests/backend/settings.rs"]
mod tests;
