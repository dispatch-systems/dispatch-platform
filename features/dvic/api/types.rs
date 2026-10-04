use dispatch_core::collection::api::jobs::PublicJob;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/dvic/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DvicReport {
    pub name: String,
    pub week: String,
    pub report_date: String,
    pub row_count: usize,
    pub short_count: usize,
    pub min_date: Option<String>,
    pub max_date: Option<String>,
    pub checked_at: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/dvic/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DvicWeek {
    pub week: String,
    pub checked_at: String,
    pub report_count: usize,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/dvic/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DvicStatus {
    pub station: String,
    pub latest_week: String,
    pub short_inspections: usize,
    pub weeks: Vec<DvicWeek>,
    pub reports: Vec<DvicReport>,
    pub jobs: Vec<PublicJob>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/dvic/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DvicInspection {
    pub id: String,
    pub start_date: String,
    pub driver_id: String,
    pub driver_name: String,
    pub vin: String,
    pub fleet_type: String,
    pub inspection_type: String,
    pub status: String,
    /// Source wall time; no UTC offset is supplied by Amazon.
    pub start_time: String,
    pub end_time: String,
    pub duration_seconds: f64,
    pub minimum_seconds: usize,
    pub short_by_seconds: f64,
    pub source_report_date: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/dvic/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct DvicInspections {
    pub inspections: Vec<DvicInspection>,
    pub next_cursor: Option<String>,
}
