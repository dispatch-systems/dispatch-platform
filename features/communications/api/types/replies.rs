//! The DSP's quick replies, as the composer offers them and Settings edits them.
use serde::{Deserialize, Serialize};

/// `GET /api/dsp/communications/quick-replies`: the DSP's quick replies, in their order, and the
/// revision a save of them goes on.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/communications/api/generated/")
)]
#[serde(default)]
pub struct QuickReplies {
    pub replies: Vec<String>,
    pub revision: i64,
}
