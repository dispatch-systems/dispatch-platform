//! The tools agents use, as the MCP server publishes them: what each is called, does, takes and
//! answers, and the feature it belongs to. Agents rely on every word of it, so a change shows in
//! review as a change to app/tests/backend/snapshots/agent-tools.json.
use crate::snapshot;
use dispatch_mcp::tools::{Effect, Scope, Toolbox};
use serde_json::{Value, json};

#[cfg(feature = "default")]
#[test]
fn the_tools_agents_use_keep_their_names_and_contracts() {
    crate::install();
    let tools: Vec<Value> = Toolbox::installed()
        .all()
        .iter()
        .map(|listed| {
            let tool = listed.tool;
            json!({
                "name": tool.name(),
                "title": tool.title(),
                "description": tool.description(),
                "feature": listed.feature.map(|feature| feature.name),
                "part": tool.part(),
                "changes": tool.effect() == Effect::Changes,
                "dsp": tool.scope() == Scope::Dsp,
                "input": tool.input_schema(),
                "output": tool.output_schema(),
            })
        })
        .collect();
    snapshot::check("agent-tools.json", &json!(tools));
}
