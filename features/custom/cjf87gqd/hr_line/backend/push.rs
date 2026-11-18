//! Telling HR Line's members of new texts to its phones with Dispatch closed, or on a phone: each
//! browser kept for HR Line, of a member who may use it and has its Notifications on, titled
//! "HR Line" before who wrote, and opening HR Line's page; never Dispatch Phone's texts, nor its
//! page.
use super::settings::DISPLAY;
use crate::LINE;
use dispatch_communications::lines::{self, Pushed};
use dispatch_core::manifest::DspMaintenance;
use std::time::Duration;

pub const PUSHED: Pushed = Pushed {
    line: Some(LINE.id),
    permission: "cjf87gqd_hr_line.use",
    page: "cjf87gqd-hr-line",
    label: Some(LINE.name),
    display: &DISPLAY,
};

/// Every minute, HR Line's DSP is watched for what to push, if a browser is kept for it, as after
/// a restart.
pub const MAINTENANCE: DspMaintenance = DspMaintenance {
    every: Duration::from_secs(60),
    run: |state, _, _| lines::pushing(state, &PUSHED),
};
