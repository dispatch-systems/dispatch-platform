//! Shared performance API datasets and raw replies.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_ROWS: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFrame {
    Weekly,
    Daily,
}
impl TimeFrame {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "Weekly",
            Self::Daily => "Daily",
        }
    }
}
/// One dataset of the performance API and the table that holds its rows.
pub struct Dataset {
    pub id: &'static str,
    pub table: &'static str,
    pub time_frame: TimeFrame,
    /// A `program` the page sends with the request.
    pub program: Option<&'static str>,
    /// Whether this request includes the station parameter.
    pub station: bool,
    /// The row field that says whether the row counts against the scorecard.
    pub impact: Option<&'static str>,
}

/// One dataset's rows for the week, as objects, with the address they came from.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetCapture {
    pub id: String,
    pub from: String,
    pub to: String,
    pub source_url: String,
    pub rows: Vec<Value>,
}

/// A dataset and the exact period a collection asks for.
pub struct Read {
    pub dataset: &'static Dataset,
    pub from: String,
    pub to: String,
}
