//! What a feature lets agents ask of it, declared in its manifest's `mcp`. The agent API,
//! the MCP server, the OpenAPI document, the Agent Skill and the Agents page are built from
//! every feature's, so each is listed once.
use super::data::catalog::{self, Endpoint, Metric, Term};
use crate::{
    contracts::{AgentArea, AgentSource},
    manifest::Feature,
};

pub struct Mcp {
    /// The kinds of data it holds that a key or app may be allowed to read.
    pub reads: &'static [AgentArea],
    /// Its switches agents read from.
    pub sources: &'static [AgentSource],
    /// Its endpoints, each also an MCP tool.
    pub endpoints: &'static [Endpoint],
    /// What `team_table` can rank by from its data.
    pub metrics: &'static [Metric],
    /// The words its answers use, for the glossary.
    pub terms: &'static [Term],
}
impl Mcp {
    pub const NONE: Self = Self {
        reads: &[],
        sources: &[],
        endpoints: &[],
        metrics: &[],
        terms: &[],
    };
}

/// Panics unless every kind and source is declared once, in a place of its own, and every
/// kind is read from a declared source, with a declared kind if any; and unless every
/// endpoint and term has a place of its own, and every endpoint and metric reads a declared
/// kind.
pub fn check(features: &[&Feature]) {
    let areas: Vec<AgentArea> = features
        .iter()
        .flat_map(|feature| feature.mcp.reads)
        .copied()
        .collect();
    let sources: Vec<AgentSource> = features
        .iter()
        .flat_map(|feature| feature.mcp.sources)
        .copied()
        .collect();
    for (index, area) in areas.iter().enumerate() {
        assert!(
            areas[..index]
                .iter()
                .all(|other| other != area && other.order() != area.order()),
            "{} repeats another kind's id or order",
            area.as_str()
        );
        assert!(
            sources.contains(&area.source()) && area.with().is_none_or(|w| areas.contains(&w)),
            "{} is read from a source, or with a kind, that is not declared",
            area.as_str()
        );
    }
    for (index, source) in sources.iter().enumerate() {
        assert!(
            sources[..index]
                .iter()
                .all(|other| other != source && other.order() != source.order()),
            "{} repeats another source's id or order",
            source.as_str()
        );
    }
    let mcp = || features.iter().map(|feature| &feature.mcp);
    let endpoints: Vec<&Endpoint> = catalog::CORE
        .iter()
        .chain(mcp().flat_map(|mcp| mcp.endpoints))
        .collect();
    for (index, endpoint) in endpoints.iter().enumerate() {
        assert!(
            endpoints[..index]
                .iter()
                .all(|other| other.order != endpoint.order),
            "{} repeats another endpoint's order",
            endpoint.id
        );
        assert!(
            endpoint.area.is_none_or(|area| areas.contains(&area)),
            "{} reads a kind of data that is not declared",
            endpoint.id
        );
    }
    for metric in mcp().flat_map(|mcp| mcp.metrics) {
        assert!(
            areas.contains(&metric.area),
            "{} comes from a kind of data that is not declared",
            metric.name
        );
    }
    let terms: Vec<&Term> = catalog::CORE_TERMS
        .iter()
        .chain(mcp().flat_map(|mcp| mcp.terms))
        .collect();
    for (index, term) in terms.iter().enumerate() {
        assert!(
            terms[..index].iter().all(|other| other.order != term.order),
            "{} repeats another term's order",
            term.term
        );
    }
}
