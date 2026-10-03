//! What agents can ask of DVIC: each driver's vehicle inspections.
mod catalog;
mod facts;
pub mod views;

use crate::{
    agents::Mcp,
    contracts::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "dvic",
    switch: "DVIC",
    order: 40,
    features: &["dvic"],
    key: "dvic",
    fresh: facts::fresh,
});
pub const DVIC: AgentArea = AgentArea::new(&ReadToggle {
    id: "dvic",
    label: "DVIC inspections",
    order: 50,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    reads: &[DVIC],
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    daily: &[&facts::InspectionDays],
};
