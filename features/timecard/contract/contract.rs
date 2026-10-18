//! Timecard's timecards, as any feature or agent tool reads them without depending on Timecard:
//! each driver's day as Paycom records it, with its punches, the meal breaks punched, and the
//! time worked, and the days collected. A Paycom period lists every day in it, so a day after
//! its collection is unknown, as is a day outside what's collected; the day of the collection
//! is only so far; and a day before it a driver has no timecard on was a day off. Timecard
//! exposes it as `timecards`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads a DSP's timecards, as Timecard keeps them.
pub trait Timecards: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The days collected of everyone, or none before the first such collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>>;
    /// The timecards under a driver's `ids` from `from` through `to`, YYYY-MM-DD in the DSP's
    /// own time, each ID matched exactly; no IDs read no timecards. Timecard refuses a day that
    /// isn't YYYY-MM-DD with `invalid_date`; and with `invalid_input`, `from` after `to`, over
    /// 31 days, over 16 IDs, or an ID that's empty or over 128 characters.
    fn timecards(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Days>;
}

/// The days collected, YYYY-MM-DD in the DSP's own time. The last is the day of the newest
/// collection, its punches only those made by then.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    pub from: String,
    pub through: String,
}

/// A driver's timecards over the days asked for.
#[derive(Clone, Debug)]
pub struct Days {
    /// Each day with punches or hours, by day and then ID. A day neither unknown nor partial
    /// missing from it had neither: a day off.
    pub days: Vec<Day>,
    /// The days asked for that no collection has reached: unknown, rather than days off.
    pub unknown: Vec<String>,
    /// The days a collection read while they were under way: a driver's day there is so far,
    /// and one missing from it may not be a day off.
    pub partial: Vec<String>,
}

/// A driver's timecard for one day.
#[derive(Clone, Debug)]
pub struct Day {
    /// Its day, YYYY-MM-DD in the DSP's own time.
    pub date: String,
    /// Which of the driver's IDs it's under.
    pub id: String,
    /// Paycom's word for it, such as "Complete", "Missing punch" or "No punches".
    pub status: String,
    /// The hours Paycom reports, which leave out meals and may include paid leave.
    pub hours: f64,
    /// The minutes on the clock between complete punches, meals left out; none when the
    /// punches can't say, as when one is missing or a clock fell in a daylight-saving change.
    pub worked_minutes: Option<i32>,
    /// Every punch, in order.
    pub punches: Vec<Punch>,
    /// Each meal break, as punched out and back in.
    pub meals: Vec<Meal>,
    /// Whether the punches need someone to look: unlabeled, unreadable or out of order.
    pub review: bool,
}

/// One punch on a timecard.
#[derive(Clone, Debug)]
pub struct Punch {
    pub kind: PunchKind,
    /// Its clock, HH:MM in the DSP's own time, or none when Paycom's isn't one.
    pub time: Option<String>,
    /// Whether it fell on a day after the timecard's, as at the end of a night's shift.
    pub next_day: bool,
}

/// What a punch says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PunchKind {
    /// In for the day.
    InDay,
    /// Out for a meal.
    OutLunch,
    /// Back in from a meal.
    InLunch,
    /// Out for the day.
    OutDay,
    /// One Paycom didn't label.
    Unlabeled,
}

/// A meal break, as punched.
#[derive(Clone, Debug)]
pub struct Meal {
    /// When it started and ended, HH:MM in the DSP's own time; none for a punch missing.
    pub out: Option<String>,
    pub back: Option<String>,
    /// How long it was; none unless both punches are there and in order.
    pub minutes: Option<i32>,
}

/// What reads the DSP's timecards: only while Timecard is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn Timecards>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn Timecards>(db, dsp)
}
