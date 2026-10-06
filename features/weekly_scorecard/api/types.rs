use serde::{Deserialize, Serialize};

/// How many rows one dataset of a publication holds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/weekly_scorecard/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyScorecardDatasetCount {
    pub id: String,
    pub table: String,
    pub rows: usize,
}
/// One collection of a week's scorecard.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/weekly_scorecard/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyScorecardPublication {
    pub id: String,
    pub week: String,
    pub station: String,
    pub dsp_code: String,
    pub collected_at: String,
    pub row_count: usize,
    pub datasets: Vec<WeeklyScorecardDatasetCount>,
}
/// What the last collection of a week found, and the active publication if it was posted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/weekly_scorecard/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyScorecardWeek {
    pub week: String,
    pub posted: bool,
    pub checked_at: String,
    pub publication: Option<WeeklyScorecardPublication>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/weekly_scorecard/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyScorecardWeeks {
    pub station: String,
    /// The most recent completed week, which a collection targets by default.
    pub latest_week: String,
    pub weeks: Vec<WeeklyScorecardWeek>,
}

/// Which completed weeks a scheduled run refreshes, independently of its wall-clock times.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/weekly_scorecard/api/generated/")
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WeeklyScorecardPolicy {
    pub lookback_weeks: u32,
    pub refresh_hours: u32,
}
impl Default for WeeklyScorecardPolicy {
    fn default() -> Self {
        Self {
            lookback_weeks: 1,
            refresh_hours: 20,
        }
    }
}
