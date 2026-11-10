//! Shared performance API datasets and raw replies.
use dispatch_core::text_enum;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_ROWS: usize = 50_000;

text_enum! {
    /// How often a dataset's rows come, as the performance API names it.
    pub enum TimeFrame {
        Weekly => "Weekly",
        Daily => "Daily",
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

/// Whether `row` names no other station, company or DSP than these, where it names one.
pub(crate) fn names_only(row: &Value, station: &str, company_id: &str, dsp_code: &str) -> bool {
    let named = |field: &str| row[field].as_str().filter(|value| !value.is_empty());
    named("station_code").is_none_or(|value| value == station)
        && named("station").is_none_or(|value| value == station)
        && named("company_id").is_none_or(|value| value == company_id)
        && named("dsp_code").is_none_or(|value| value.eq_ignore_ascii_case(dsp_code))
}

/// A dataset and the exact period a collection asks for.
pub struct Read {
    pub dataset: &'static Dataset,
    pub from: String,
    pub to: String,
}
