//! What agents can ask of Weekly Scorecard: Amazon's weekly scorecard, its customer feedback,
//! Netradyne safety events and returns to station.
mod catalog;
mod performance;
mod query;
mod synthetic;
pub mod weekly_scorecard;

use dispatch_core::mcp::{
    Mcp,
    api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "weekly_scorecard",
    switch: "Weekly Scorecard",
    label: "Weekly Scorecard",
    order: 50,
    features: &["weekly_scorecard"],
    key: "weekly_scorecard",
    fresh: weekly_scorecard::fresh,
});
pub const FEEDBACK: AgentArea = AgentArea::new(&ReadToggle {
    id: "feedback",
    label: "Customer feedback",
    hint: None,
    missing: "customer feedback",
    order: 60,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub const SAFETY: AgentArea = AgentArea::new(&ReadToggle {
    id: "safety",
    label: "Safety events",
    hint: None,
    missing: "safety events",
    order: 70,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub const RETURNS: AgentArea = AgentArea::new(&ReadToggle {
    id: "returns",
    label: "Returns & contact compliance",
    hint: None,
    missing: "returns",
    order: 80,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
pub const WEEKLY_SCORECARD: AgentArea = AgentArea::new(&ReadToggle {
    id: "weekly_scorecard",
    label: "Weekly Scorecard",
    hint: None,
    missing: "weekly scorecard",
    order: 90,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    instructions: "- weekly_scorecard defaults to the latest week. Focused feedback, returns and safety tools \
        default to this weekly source, with the last 30 days as their default period. \
        View posted_scorecard and compare default to last week. Posted scoring and disputes are authoritative; \
        weekly package feedback and DSP response counters are distinct measures.",
    reads: &[FEEDBACK, SAFETY, RETURNS, WEEKLY_SCORECARD],
    missing: "weekly scorecard data",
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    performance: performance::ADAPTERS,
    examples: catalog::EXAMPLES,
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
