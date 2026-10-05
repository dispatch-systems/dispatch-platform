//! What a feature lets agents ask of it, declared in its manifest's `mcp`. The agent API,
//! the MCP server, the OpenAPI document, the Agent Skill and the Agents page are built from
//! every feature's, so each is listed once.
use super::{
    data::{
        catalog::{self, Endpoint, Metric, Term},
        facts::{Daily, Places},
        scope::Identify,
    },
    skill::{self, Example},
    synthetic::{Step, Synthetic},
};
use crate::{
    manifest::Feature,
    mcp::api::types::{AgentArea, AgentSource},
};

pub struct Mcp {
    /// The kinds of data it holds that a key or app may be allowed to read. The Agents page
    /// lists their switches under the feature's name.
    pub reads: &'static [AgentArea],
    /// How a key's row on the Agents page names all of its kinds together, when the key
    /// reads none of them.
    pub missing: &'static str,
    /// Its switches agents read from.
    pub sources: &'static [AgentSource],
    /// Its endpoints, each also an MCP tool.
    pub endpoints: &'static [Endpoint],
    /// Previous paths served through the canonical endpoint, omitted from discovery.
    pub legacy_paths: &'static [(&'static str, &'static str)],
    /// What `team_table` can rank by from its data.
    pub metrics: &'static [Metric],
    /// The words its answers use, for the glossary.
    pub terms: &'static [Term],
    /// Questions its data answers, with the calls that answer them, for the Agent Skill.
    pub examples: &'static [Example],
    /// Its facts by driver and day, for the answers that join every feature's.
    pub daily: &'static [&'static dyn Daily],
    /// Where packages were delivered, for the answers of other features that place what
    /// they count.
    pub places: Option<Places>,
    /// Who the people its sources name are, for every answer that names a driver: the one
    /// feature that tells people apart fills it.
    pub identity: Option<Identify>,
    /// What it holds of the synthetic DSP agents are tried against.
    pub synthetic: Synthetic,
}
impl Mcp {
    pub const NONE: Self = Self {
        reads: &[],
        missing: "",
        sources: &[],
        endpoints: &[],
        legacy_paths: &[],
        metrics: &[],
        terms: &[],
        examples: &[],
        daily: &[],
        places: None,
        identity: None,
        synthetic: Synthetic::NONE,
    };
}

/// Panics unless every kind and source is declared once, in a place of its own, and every
/// kind is read from a declared source, with a declared kind if any; and unless every
/// endpoint, term and example has a place of its own, every endpoint and metric reads a
/// declared kind, each kind has facts by driver and day once at most, and one feature at
/// most says where packages were delivered, one who people are, and one names the
/// synthetic DSP's people, whose steps each have a place of their own.
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
    let mut legacy_paths = std::collections::BTreeSet::new();
    for feature in features {
        for (path, id) in feature.mcp.legacy_paths {
            assert!(
                legacy_paths.insert(path) && endpoints.iter().all(|e| e.path != *path),
                "{path} repeats an agent route"
            );
            assert!(
                feature.mcp.endpoints.iter().any(|e| e.id == *id),
                "{} aliases an endpoint it does not own: {id}",
                feature.name
            );
        }
    }
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
    assert!(
        mcp().filter(|mcp| mcp.places.is_some()).count() <= 1,
        "more than one feature answers where packages were delivered"
    );
    assert!(
        mcp().filter(|mcp| mcp.identity.is_some()).count() <= 1,
        "more than one feature says who people are"
    );
    assert!(
        mcp().filter(|mcp| mcp.synthetic.people.is_some()).count() <= 1,
        "more than one feature names the synthetic DSP's people"
    );
    let steps: Vec<&Step> = mcp().flat_map(|mcp| mcp.synthetic.steps).collect();
    for (index, step) in steps.iter().enumerate() {
        assert!(
            steps[..index].iter().all(|other| other.order != step.order),
            "two steps of the synthetic DSP share the place {}",
            step.order
        );
    }
    let daily: Vec<AgentArea> = mcp().flat_map(|mcp| mcp.daily).map(|d| d.area()).collect();
    for (index, area) in daily.iter().enumerate() {
        assert!(
            areas.contains(area) && !daily[..index].contains(area),
            "{} has facts by driver and day that are undeclared or declared twice",
            area.as_str()
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
    let examples: Vec<&Example> = skill::CORE_EXAMPLES
        .iter()
        .chain(mcp().flat_map(|mcp| mcp.examples))
        .collect();
    for (index, example) in examples.iter().enumerate() {
        assert!(
            examples[..index]
                .iter()
                .all(|other| other.order != example.order),
            "{} repeats another example's order",
            example.question
        );
    }
}
