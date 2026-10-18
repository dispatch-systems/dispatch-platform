//! DVIC's short inspections, as any feature or agent tool reads them without depending on DVIC:
//! the pre-trip inspections that fell short of the minimum time, the drivers who made them, and
//! the days collected. DVIC records only the inspections that fell short, so a driver it never
//! names was never short in what it has collected, and a day outside what's collected is
//! unknown. DVIC exposes it as `short_inspections`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads a DSP's short inspections, as DVIC keeps them.
pub trait ShortInspections: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The days collected, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>>;
    /// Every driver with a short inspection, by name.
    fn drivers(&self, db: &Store, dsp: &str) -> Result<Vec<Driver>>;
    /// The short inspections `asked` names, a page at a time.
    fn inspections(&self, db: &Store, dsp: &str, asked: &Asked<'_>) -> Result<Page>;
}

/// The days collected, YYYY-MM-DD in the DSP's own time.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    pub from: String,
    pub through: String,
}

/// A driver with a short inspection.
#[derive(Clone, Debug)]
pub struct Driver {
    /// Their ID in `source`.
    pub id: String,
    /// The name the inspections give them.
    pub name: String,
    /// The day of their first short inspection, YYYY-MM-DD.
    pub first_date: String,
}

/// Which short inspections to read, a page at a time. DVIC refuses with `invalid_input` a day
/// that isn't YYYY-MM-DD, `from` after `to`, a driver's ID over 128 characters, `after` over
/// 256, or a `limit` outside 1 to 500.
pub struct Asked<'a> {
    /// The first day and the last, YYYY-MM-DD in the DSP's own time.
    pub from: &'a str,
    pub to: &'a str,
    /// One driver's, by ID, or everyone's.
    pub driver: Option<&'a str>,
    /// Where the page before ended, as its `next` said, or empty for the first.
    pub after: &'a str,
    /// The most a page holds.
    pub limit: usize,
}

/// One page of short inspections, and where the next starts, if there is one.
#[derive(Clone, Debug)]
pub struct Page {
    pub inspections: Vec<Inspection>,
    pub next: Option<String>,
}

/// A pre-trip inspection shorter than the minimum.
#[derive(Clone, Debug)]
pub struct Inspection {
    /// Its day, YYYY-MM-DD in the DSP's own time.
    pub date: String,
    /// When it started, as Amazon writes it, with no offset.
    pub start_time: String,
    pub driver_id: String,
    pub driver_name: String,
    /// The vehicle's type, as Amazon names its fleets.
    pub vehicle_type: String,
    pub vin: String,
    pub duration_seconds: f64,
    pub minimum_seconds: usize,
    pub short_by_seconds: f64,
}

/// What reads the DSP's short inspections: only while DVIC is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn ShortInspections>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn ShortInspections>(db, dsp)
}
