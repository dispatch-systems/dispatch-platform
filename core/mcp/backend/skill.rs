//! The Dispatch Agent Skill: one `SKILL.md`, in the open Agent Skills format that Claude
//! Code, Codex, Hermes and others load. It is written from the catalog, as the tools and the
//! OpenAPI document are, so it always describes the API this server answers.
use super::data::catalog::{ENDPOINTS, Endpoint, GLOSSARY, METRICS};
use crate::{manifest::registry, mcp::server::INSTRUCTIONS};
use std::sync::LazyLock;

/// A question people ask, with the call and the REST request that answer it.
pub struct Example {
    pub question: &'static str,
    /// The tool and its arguments, as `tool(name: value, …)`.
    pub call: &'static str,
    pub rest: &'static str,
    /// Its place among every feature's examples in the skill.
    pub order: u16,
}

/// The questions core's own tools answer alone.
pub(crate) const CORE_EXAMPLES: &[Example] = &[Example {
    question: "How did Daniel do last week?",
    call: r#"driver_report(driver: "Daniel", period: "last week")"#,
    rest: "/api/v1/drivers/Daniel?period=last%20week",
    order: 100,
}];

/// Every example, core's and each feature's, in their order.
pub static EXAMPLES: LazyLock<Vec<&'static Example>> = LazyLock::new(|| {
    let features = registry().features.iter().flat_map(|f| f.mcp.examples);
    let mut all: Vec<&'static Example> = CORE_EXAMPLES.iter().chain(features).collect();
    all.sort_by_key(|example| example.order);
    all
});

fn endpoint(out: &mut String, endpoint: &Endpoint) {
    out.push_str(&format!(
        "### `{}` · `GET {}`\n\n{}\n\n",
        endpoint.tool, endpoint.path, endpoint.description
    ));
    for param in endpoint.path_params.iter().chain(endpoint.params) {
        out.push_str(&format!("- `{}`: {}\n", param.name, param.description));
    }
    if endpoint.path_params.is_empty() && endpoint.params.is_empty() {
        out.push_str("- No parameters.\n");
    }
    out.push('\n');
}

/// The skill, naming this server's address.
pub fn skill(origin: &str) -> String {
    let mut out = String::from(
        "---\n\
         name: dispatch\n\
         description: Answers questions about a delivery service partner's drivers from \
         Dispatch: routes, stops and packages, hours and timecards, meal breaks and DVIC \
         vehicle inspections, customer feedback, returns, safety events, weekly scorecards and daily performance, \
         for one driver or the whole team, on any day or period. Use \
         when asked how a driver did, who led or trailed on a number, what happened on a \
         route or to a package, or about hours, lunches, inspections, feedback, safety, returns \
         or scorecard performance.\n\
         ---\n\n\
         # Dispatch\n\n",
    );
    out.push_str(INSTRUCTIONS);
    out.push_str(&format!(
        "\n\n## Connecting\n\n\
         Use the Dispatch MCP tools when they are connected. Otherwise call the REST API \
         with the key in `$DISPATCH_KEY`; every tool below is also an endpoint taking the \
         same parameters, and gives the same answer:\n\n\
         ```sh\n\
         curl --fail-with-body -sS -H \"Authorization: Bearer $DISPATCH_KEY\" \"{origin}/api/v1/whoami\"\n\
         ```\n\n\
         MCP: `{origin}/api/v1/mcp`. OpenAPI: `{origin}/api/v1/openapi.json`.\n\n\
         ## Questions and the calls that answer them\n\n\
         | Question | Tool | REST |\n| --- | --- | --- |\n"
    ));
    for example in EXAMPLES.iter() {
        out.push_str(&format!(
            "| {} | `{}` | `GET {}` |\n",
            example.question, example.call, example.rest
        ));
    }
    out.push_str("\n## Tools\n\n");
    for each in ENDPOINTS.iter() {
        endpoint(&mut out, each);
    }
    out.push_str("## Metrics for `team_table`\n\n");
    for metric in METRICS.iter() {
        let per = if metric.total == "day" {
            ", per day only"
        } else {
            ""
        };
        out.push_str(&format!(
            "- `{}` ({}, from {}{per}): {}\n",
            metric.name,
            metric.unit,
            metric.area.as_str(),
            metric.description
        ));
    }
    out.push_str("\n## Terms\n\n");
    for term in GLOSSARY.iter() {
        out.push_str(&format!("- **{}**: {}\n", term.term, term.meaning));
    }
    out
}
