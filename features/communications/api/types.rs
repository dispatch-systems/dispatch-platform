//! What Communications's endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
use serde::Serialize;

/// `GET /api/dsp/communications`: what Communications holds for the DSP in view.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct CommunicationsSummary {
    pub dsp_id: String,
    pub items: usize,
}
