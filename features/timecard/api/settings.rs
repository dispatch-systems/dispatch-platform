use crate::api::assessment::LateRule;
use dispatch_core::text_enum;
use serde::{Deserialize, Serialize};

text_enum! {
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
    pub enum NameOrder { FirstLast => "first_last", LastFirst => "last_first", }
}
/// What the Timecard page shows by, of the DSP's Timecard settings, which Settings saves: how
/// Meal Breaks writes names, the departments that drive (none for all of them), and when a
/// DA is late.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/timecard/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct PaycomSettings {
    pub name_order: NameOrder,
    pub driver_departments: Option<Vec<String>>,
    pub late: LateRule,
}
