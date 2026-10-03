//! DVIC: a station's short vehicle inspections, week by week, from the reports Cortex
//! publishes.
mod capture;
pub(crate) mod collect;

pub use capture::{
    Capture, Collection, Inspection, JOB_KIND, KnownReport, MAX_WEEKS, Request, fixture, hash,
    report_week,
};
