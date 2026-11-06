//! Daily source status and the Paycom publication the page observes.
use dispatch_core::collection::api::{jobs::PublicJob, types::Connection};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DailySyncSource {
    pub enabled: bool,
    pub active: bool,
    pub job_date: Option<String>,
    pub job: Option<PublicJob>,
    pub collected_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DailySyncStatus {
    pub date: String,
    pub scope_available: bool,
    pub paycom: DailySyncSource,
    pub flex: DailySyncSource,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PaycomWorkforce {
    pub collected_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PaycomStatus {
    pub connection: Connection,
    pub workforce: PaycomWorkforce,
}
