//! What agents can ask of Routes: each day's routes and packages, and the delivery
//! addresses with them.
mod catalog;
mod facts;
mod synthetic;
pub mod views;

use dispatch_core::mcp::{
    Mcp,
    api::types::{AgentArea, AgentSource, DriverSource, ReadSource, ReadToggle},
    data::facts::Places,
};

pub const SOURCE: AgentSource = AgentSource::new(&ReadSource {
    id: "routes",
    switch: "Routes",
    label: "Routes",
    order: 10,
    features: &["routes"],
    key: "routes",
    fresh: facts::fresh,
});
pub const ROUTES: AgentArea = AgentArea::new(&ReadToggle {
    id: "routes",
    label: "Routes & packages",
    hint: None,
    missing: "routes",
    order: 10,
    source: SOURCE,
    with: None,
    opt_in: false,
    names: &[DriverSource::Amazon],
});
/// The delivery addresses and GPS that route answers carry.
pub const LOCATIONS: AgentArea = AgentArea::new(&ReadToggle {
    id: "locations",
    label: "Delivery addresses & GPS",
    hint: Some("Stop addresses and GPS points"),
    missing: "delivery addresses",
    order: 20,
    source: SOURCE,
    with: Some(ROUTES),
    opt_in: true,
    names: &[],
});

pub const MCP: Mcp = Mcp {
    reads: &[ROUTES, LOCATIONS],
    missing: "route data",
    sources: &[SOURCE],
    endpoints: catalog::ENDPOINTS,
    metrics: catalog::METRICS,
    terms: catalog::TERMS,
    examples: catalog::EXAMPLES,
    daily: &[&facts::RouteDays],
    places: Some(Places {
        area: LOCATIONS,
        of: facts::places,
    }),
    synthetic: synthetic::SYNTHETIC,
    ..Mcp::NONE
};
