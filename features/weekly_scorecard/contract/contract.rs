//! Driver Scorecards, as any feature or agent tool uses it without depending on Weekly Scorecard:
//! TODO(scaffold): what it offers, in a sentence. Weekly Scorecard exposes it as `driver_scorecards`.
use dispatch_core::{Result, db::Store};

/// TODO(scaffold): what it reads or does for a DSP, in a sentence.
pub trait DriverScorecards: Send + Sync {
    /// Whether it's ready for the DSP. TODO(scaffold): its readers and actions instead, each
    /// with only what its first user needs, and every one naming the DSP.
    fn ready(&self, db: &Store, dsp: &str) -> Result<bool>;
}

/// What offers it for the DSP: only while Weekly Scorecard is on there.
pub fn for_dsp(db: &Store, dsp: &str) -> Result<Option<&'static dyn DriverScorecards>> {
    dispatch_core::manifest::exposes::for_dsp::<dyn DriverScorecards>(db, dsp)
}
