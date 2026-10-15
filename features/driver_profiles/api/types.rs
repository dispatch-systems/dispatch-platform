//! What Driver Profiles' endpoints answer with. Each type is written to TypeScript in
//! `generated/`, which `client.ts` imports.
use dispatch_core::collection::api::jobs::PublicJob;
use serde::Serialize;

/// `GET /api/dsp/driver-profiles/status`: when the roster was collected, what it holds,
/// and when it is collected next.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts",
    ts(export_to = "features/driver_profiles/api/generated/")
)]
#[serde(rename_all = "camelCase")]
pub struct RosterStatus {
    pub collected_at: Option<String>,
    /// Associates the roster lists as active.
    pub active: usize,
    /// Associates it lists as inactive.
    pub inactive: usize,
    /// Photos kept.
    pub photos: usize,
    /// Photos the last collection left for the next.
    pub photos_waiting: usize,
    /// When a schedule collects it next.
    pub next_run: Option<String>,
    /// Its recent collections, the newest first.
    pub jobs: Vec<PublicJob>,
}
