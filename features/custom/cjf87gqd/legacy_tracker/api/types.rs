//! What Legacy Tracker's endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
use serde::Serialize;

/// `GET /api/dsp/cjf87gqd-legacy-tracker`: the address the DSP's link opens, none until it sets
/// one.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/custom/cjf87gqd/legacy_tracker/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct DspCjf87gqdLegacyTrackerSummary {
    pub link: Option<String>,
}
