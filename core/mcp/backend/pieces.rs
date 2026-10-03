//! What a feature lets agents ask of it, declared in its manifest's `mcp`. The agent API,
//! the MCP server, the OpenAPI document, the Agent Skill and the Agents page are built from
//! every feature's, so each is listed once.
use crate::{
    contracts::{AgentArea, AgentSource},
    manifest::Feature,
};

pub struct Mcp {
    /// The kinds of data it holds that a key or app may be allowed to read.
    pub reads: &'static [AgentArea],
    /// Its switches agents read from.
    pub sources: &'static [AgentSource],
}
impl Mcp {
    pub const NONE: Self = Self {
        reads: &[],
        sources: &[],
    };
}

/// Panics unless every kind and source is declared once, in a place of its own, and every
/// kind is read from a declared source, with a declared kind if any.
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
}
