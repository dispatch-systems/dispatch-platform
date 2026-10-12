//! Exact period-batch status, independent of the employee selected in the dashboard.
use dispatch_core::collection::api::jobs::PublicJob;
use dispatch_paycom::timecards::EmployeeTimecardPeriod;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct EmployeeSyncSource {
    pub enabled: bool,
    pub active: bool,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub progress: u8,
    pub jobs: Vec<PublicJob>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct EmployeeSyncStatus {
    pub period: EmployeeTimecardPeriod,
    pub through: String,
    pub request_id: Option<String>,
    pub scope_available: bool,
    /// Any unfinished collection for this DSP holds its manual-sync lock.
    pub active: bool,
    pub complete: bool,
    pub paycom: EmployeeSyncSource,
    pub cortex: EmployeeSyncSource,
}
