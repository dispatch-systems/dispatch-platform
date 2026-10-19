//! Daily publication contracts.
use serde::Serialize;
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/daily_performance/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct DailyPerformanceDataset {
    pub id: String,
    pub dataset: String,
    pub rows: usize,
    /// observed when rows exist; unconfirmed when Amazon returned none.
    pub coverage: String,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/daily_performance/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct DailyPerformanceDay {
    pub date: String,
    pub collected_at: String,
    pub row_count: usize,
    pub datasets: Vec<DailyPerformanceDataset>,
}
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/daily_performance/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct DailyPerformanceSummary {
    pub station: String,
    pub latest_date: String,
    pub days: Vec<DailyPerformanceDay>,
}
