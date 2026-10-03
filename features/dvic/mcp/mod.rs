//! What agents can ask of DVIC: each driver's vehicle inspections.
use crate::{
    agents::Mcp,
    contracts::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "dvic",
    switch: "DVIC",
    order: 40,
    features: &["dvic"],
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
};
