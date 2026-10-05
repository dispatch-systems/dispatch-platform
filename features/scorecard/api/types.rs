use serde::Serialize;

/// How many rows one dataset of a publication holds.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/scorecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScorecardDatasetCount {
    pub id: String,
    pub table: String,
    pub rows: usize,
}
/// One collection of a week's scorecard.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/scorecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScorecardPublication {
    pub id: String,
    pub week: String,
    pub station: String,
    pub dsp_code: String,
    pub collected_at: String,
    pub row_count: usize,
    pub datasets: Vec<ScorecardDatasetCount>,
}
/// What the last collection of a week found, and the active publication if it was posted.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/scorecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScorecardWeek {
    pub week: String,
    pub posted: bool,
    pub checked_at: String,
    pub publication: Option<ScorecardPublication>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/scorecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct ScorecardWeeks {
    pub station: String,
    /// The most recent completed week, which a collection targets by default.
    pub latest_week: String,
    pub weeks: Vec<ScorecardWeek>,
}
