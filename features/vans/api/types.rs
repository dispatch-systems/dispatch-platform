//! What Vans' endpoints answer with. Each type is written to TypeScript in `generated/`,
//! which `client.ts` imports.
use dispatch_core::collection::api::jobs::PublicJob;
use serde::Serialize;

/// `GET /api/dsp/vans/status`: when the fleet was collected, what it holds, and when it is
/// collected next.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "features/vans/api/generated/"))]
#[serde(rename_all = "camelCase")]
pub struct FleetStatus {
    pub collected_at: Option<String>,
    /// Vans the fleet lists.
    pub listed: usize,
    /// When a schedule collects it next.
    pub next_run: Option<String>,
    /// Its recent collections, the newest first.
    pub jobs: Vec<PublicJob>,
}
