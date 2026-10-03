//! What agents can ask of Routes: each day's routes and packages, and the delivery
//! addresses with them.
mod catalog;
mod facts;
mod synthetic;
pub mod views;

use crate::{
    agents::{Mcp, data::facts::Places},
    contracts::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "routes",
    switch: "Routes",
    order: 10,
    features: &["routes"],
    key: "routes",
    fresh: facts::fresh,
});
pub const ROUTES: AgentArea = AgentArea::new(&ReadToggle {
    id: "routes",
    label: "Routes & packages",
    order: 10,
    source: SOURCE,
    with: None,
    names: &[DriverSource::Amazon],
});
/// The delivery addresses and GPS that route answers carry.
pub const LOCATIONS: AgentArea = AgentArea::new(&ReadToggle {
    id: "locations",
    label: "Delivery addresses & GPS",
    order: 20,
    source: SOURCE,
    with: Some(ROUTES),
    names: &[],
});

pub const MCP: Mcp = Mcp {
    reads: &[ROUTES, LOCATIONS],
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    daily: &[&facts::RouteDays],
    places: Some(Places {
        area: LOCATIONS,
        of: facts::places,
    }),
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
