//! What agents can ask of Daily Performance: its endpoint, and the kind of data a key or app may be
//! allowed to read. The MCP server, the OpenAPI document, the skill and the Agents page's read
//! toggles are built from it.
mod catalog;
mod fields;
mod performance;
mod synthetic;
mod variants;
mod views;

use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::Store,
    mcp::{
        Mcp,
        api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
    },
};
use serde_json::Value;

/// Daily Performance's switch, as agents read from it.
pub(crate) const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "daily_performance",
    switch: "Daily Performance",
    label: "Daily Performance",
    order: 180,
    features: &["daily_performance"],
    key: "daily_performance",
    fresh,
});
/// The read toggle a key or app needs to read Daily Performance. Permanent: keys and apps store it.
pub(crate) const DAILY_PERFORMANCE: AgentArea = AgentArea::new(&ReadToggle {
    id: "daily_performance",
    label: "Daily Performance",
    hint: None,
    missing: "daily performance",
    order: 180,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});

pub(crate) const DAILY_FEEDBACK: AgentArea = AgentArea::new(&ReadToggle {
    id: "daily_feedback",
    label: "Daily customer feedback",
    hint: None,
    missing: "daily customer feedback",
    order: 181,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub(crate) const DAILY_RETURNS: AgentArea = AgentArea::new(&ReadToggle {
    id: "daily_returns",
    label: "Daily returns & contact compliance",
    hint: None,
    missing: "daily returns & contact compliance",
    order: 182,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub(crate) const DAILY_SAFETY: AgentArea = AgentArea::new(&ReadToggle {
    id: "daily_safety",
    label: "Daily safety events",
    hint: None,
    missing: "daily safety events",
    order: 183,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub(crate) const MCP: Mcp = Mcp {
    about: "each day's delivery quality and safety",
    asked: "about a day's quality or safety",
    instructions: "- Daily Performance defaults to yesterday. Choose source daily_performance or view operational for daily questions. \
        Operational reconciles return attempts and assessed/live safety; live-only events are pending. \
        Feedback is daily response counts, not package reviews. Daily annotations do not establish posted scoring.",
    reads: &[
        DAILY_PERFORMANCE,
        DAILY_FEEDBACK,
        DAILY_RETURNS,
        DAILY_SAFETY,
    ],
    missing: "daily performance data",
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    variants: variants::ENDPOINTS,
    performance: performance::ADAPTERS,
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};

/// When it last brought something in for the DSP, for the status answer.
fn fresh(db: &Store, dsp: &Dsp, station: &str) -> Result<Option<Value>> {
    use crate::DailyPerformanceStore;
    db.daily_performance_db(&dsp.id)?.one(
        "SELECT max(date) latestDay,max(collected_at) collectedAt FROM daily_publications WHERE station=? AND active=1", [station])
        .map(|row| row.filter(|row| !row["latestDay"].is_null()))
}
