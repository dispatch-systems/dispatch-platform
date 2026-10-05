//! What agents can ask of Scorecard: Amazon's weekly scorecard, its customer feedback,
//! Netradyne safety events and returns to station.
mod catalog;
mod query;
pub mod scorecard;
mod synthetic;

use dispatch_core::mcp::{
    Mcp,
    api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "scorecard",
    switch: "Scorecard",
    label: "Scorecard",
    order: 50,
    features: &["scorecard"],
    key: "scorecard",
    fresh: scorecard::fresh,
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
pub const SCORECARD: AgentArea = AgentArea::new(&ReadToggle {
    id: "scorecard",
    label: "Weekly scorecard",
    hint: None,
    missing: "weekly scorecard",
    order: 90,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    reads: &[FEEDBACK, SAFETY, RETURNS, SCORECARD],
    missing: "scorecard data",
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    examples: catalog::EXAMPLES,
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
