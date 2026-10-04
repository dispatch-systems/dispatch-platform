//! What agents can ask of DVIC: each driver's vehicle inspections.
mod catalog;
mod facts;
mod synthetic;
pub mod views;

use dispatch_core::mcp::{
    Mcp,
    api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "dvic",
    switch: "DVIC",
    label: "DVIC",
    order: 40,
    features: &["dvic"],
    key: "dvic",
    fresh: facts::fresh,
});
pub const DVIC: AgentArea = AgentArea::new(&ReadToggle {
    id: "dvic",
    label: "DVIC inspections",
    hint: None,
    missing: "DVIC inspections",
    order: 50,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});

pub const MCP: Mcp = Mcp {
    reads: &[DVIC],
    missing: "DVIC inspections",
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    examples: catalog::EXAMPLES,
    daily: &[&facts::InspectionDays],
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
