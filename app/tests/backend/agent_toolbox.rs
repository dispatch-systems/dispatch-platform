//! The tools agents use, as the MCP server publishes them: what each is called, does, takes and
//! answers, what it is about, and the features it needs. Agents rely on every word of it, so a
//! change shows in review as a change to app/tests/backend/snapshots/agent-tools.json.
use crate::snapshot;
use dispatch_mcp::toolbox::{Effect, Scope, Toolbox};
use serde_json::{Value, json};

#[cfg(feature = "default")]
#[test]
fn the_tools_agents_use_keep_their_names_and_contracts() {
    crate::install();
    let tools: Vec<Value> = Toolbox::installed()
        .all()
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name(),
                "title": tool.title(),
                "description": tool.description(),
                "features": tool.features(),
                "changes": tool.effect() == Effect::Changes,
                "scope": match tool.scope() {
                    Scope::Dsp => "dsp",
                    Scope::Dsps => "dsps",
                    Scope::Connection => "connection",
                },
                "input": tool.input_schema(),
                "output": tool.output_schema(),
            })
        })
        .collect();
    snapshot::check("agent-tools.json", &json!(tools));
}

// A tool that needs a feature this build leaves out isn't offered in it; in the whole
// product, every tool is, so a feature misnamed by a tool fails here.
#[cfg(feature = "default")]
#[test]
fn every_tool_needs_only_features_the_product_has() {
    crate::install();
    let offered: Vec<&str> = Toolbox::installed()
        .all()
        .iter()
        .map(|tool| tool.name())
        .collect();
    let declared: Vec<&str> = dispatch_mcp::tools::TOOLS
        .iter()
        .map(|tool| tool.name())
        .collect();
    assert_eq!(offered, declared);
}
