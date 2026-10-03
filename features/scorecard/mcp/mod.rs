//! What agents can ask of Scorecard: Amazon's weekly scorecard, its customer feedback,
//! Netradyne safety events and returns to station.
mod catalog;
pub mod scorecard;

use crate::{
    agents::Mcp,
    contracts::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "scorecard",
    switch: "Scorecard",
    order: 50,
    features: &["scorecard"],
    key: "scorecard",
    fresh: scorecard::fresh,
});
pub const FEEDBACK: AgentArea = AgentArea::new(&ReadToggle {
    id: "feedback",
    label: "Customer feedback",
    order: 60,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});
pub const SAFETY: AgentArea = AgentArea::new(&ReadToggle {
    id: "safety",
    label: "Safety events",
    order: 70,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});
pub const RETURNS: AgentArea = AgentArea::new(&ReadToggle {
    id: "returns",
    label: "Returns & contact compliance",
    order: 80,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});
pub const SCORECARD: AgentArea = AgentArea::new(&ReadToggle {
    id: "scorecard",
    label: "Weekly scorecard",
    order: 90,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    reads: &[FEEDBACK, SAFETY, RETURNS, SCORECARD],
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    ..Mcp::NONE
};
