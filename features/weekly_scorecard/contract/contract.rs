//! Drivers' weekly scorecards, as any feature or agent tool reads them without depending on
//! Weekly Scorecard: each driver's scorecard for a week, the packages it lists for them, and the
//! weeks collected. A scorecard week runs Sunday to Saturday, and a scorecard or package counts in
//! the days asked when its week overlaps them, even a package whose own day is outside them. A
//! week outside what's collected is unknown. Weekly Scorecard exposes it as `driver_scorecards`.
use dispatch_core::{Result, db::Store, manifest::people::Source};

/// Reads a DSP's drivers' weekly scorecards, as Weekly Scorecard keeps them. It refuses a day
/// that isn't YYYY-MM-DD with `invalid_date`; and with `invalid_input`, `from` after `to`, more
/// than 366 days, more than 32 IDs, or an ID that's empty or over 128 characters. An ID matches
/// exactly as written.
pub trait DriverScorecards: Send + Sync {
    /// The source whose IDs name its drivers, to find them among the DSP's people.
    fn source(&self) -> Source;
    /// The weeks collected, or none before the first collection.
    fn collected(&self, db: &Store, dsp: &str) -> Result<Option<Collected>>;
    /// The scorecards of the weeks overlapping `from` to `to` that name any of `ids`, the oldest
    /// week first.
    fn scorecards(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<Scorecard>>;
    /// The packages the scorecards of the weeks overlapping `from` to `to` list for any of `ids`,
    /// the oldest week first, then by day.
    fn packages(
        &self,
        db: &Store,
        dsp: &str,
        ids: &[String],
        from: &str,
        to: &str,
    ) -> Result<Vec<Package>>;
}

/// The weeks collected.
#[derive(Clone, Debug, PartialEq)]
pub struct Collected {
    /// The first week's Sunday and the last week's Saturday, YYYY-MM-DD.
    pub from: String,
    pub through: String,
    /// Every week posted and collected, as Amazon names them (2026-W38), the oldest first. A
    /// week between `from` and `through` that isn't here is unknown.
    pub weeks: Vec<String>,
}

/// A driver's scorecard for one week.
#[derive(Clone, Debug)]
pub struct Scorecard {
    /// Its week, as Amazon names it (2026-W38).
    pub week: String,
    /// Its week's Sunday and Saturday, YYYY-MM-DD.
    pub from: String,
    pub through: String,
    /// Their ID in `source`.
    pub driver_id: String,
    /// The name the scorecard gives them.
    pub name: String,
    /// The overall score, and its tier as Amazon writes it, such as Platinum.
    pub overall_score: Option<f64>,
    pub overall_tier: Option<String>,
    /// The packages they delivered that week.
    pub delivered: Option<i64>,
    /// Each metric it grades, in the scorecard's order.
    pub metrics: Vec<Metric>,
}

/// A metric a scorecard grades.
#[derive(Clone, Debug)]
pub struct Metric {
    /// Its name: seatbelt, speeding, distractions, following_distance, sign_signal_violations,
    /// cdf_dpmo, ced, rts, dsb or pod.
    pub name: String,
    /// What it measured, a rate, DPMO or count, with its tier and score, as Amazon writes them.
    pub value: Option<f64>,
    pub tier: Option<String>,
    pub score: Option<f64>,
}

/// A package a scorecard lists for a driver.
#[derive(Clone, Debug)]
pub struct Package {
    pub kind: PackageKind,
    /// The scorecard week it counts in.
    pub week: String,
    /// Its own day, YYYY-MM-DD in the DSP's own time: the day delivered for a concession or
    /// feedback, or planned for a return. None when Amazon leaves it out.
    pub date: Option<String>,
    /// Its tracking ID, as Amazon writes it (TBA…).
    pub tracking_id: String,
    pub driver_id: String,
    /// Whether it counts against the scorecard, or none when Amazon doesn't say.
    pub impacting: Option<bool>,
    /// What happened, in Amazon's words: the flags a concession raised, a return's reason, or the
    /// answers a customer marked.
    pub reasons: Vec<String>,
}

/// The scorecard list a package is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    /// Delivered, then reported missing or delivered badly (DNR and Delivery Success Behaviors).
    Concession,
    /// Brought back to the station (Delivery Completion).
    Return,
    /// Feedback from the customer, good or bad.
    Feedback,
}

/// What reads the DSP's drivers' scorecards: only while Weekly Scorecard is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn DriverScorecards>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn DriverScorecards>(db, dsp)
}
