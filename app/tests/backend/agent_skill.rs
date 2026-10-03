//! The agent skill names every registered feature's tools and metrics.
use crate::agents::{
    data::catalog::{ENDPOINTS, METRICS},
    skill::{EXAMPLES, skill},
};
#[test]
fn the_skill_names_every_tool_and_metric() {
    let skill = skill("https://dispatch.example.com");
    assert!(skill.starts_with("---\nname: dispatch\ndescription: "));
    let description = skill.lines().nth(2).unwrap();
    assert!(description.len() <= 1024 + "description: ".len());
    for endpoint in ENDPOINTS.iter() {
        assert!(
            skill.contains(&format!("`{}`", endpoint.tool)),
            "{}",
            endpoint.tool
        );
    }
    for metric in METRICS.iter() {
        assert!(
            skill.contains(&format!("`{}`", metric.name)),
            "{}",
            metric.name
        );
    }
    // Every example calls a tool that exists, with parameters it takes.
    for (_, call, _) in EXAMPLES {
        let (name, args) = call.split_once('(').unwrap();
        let endpoint = crate::agents::data::catalog::tool(name).unwrap();
        for arg in args.trim_end_matches(')').split(", ") {
            let param = arg.split(':').next().unwrap().trim();
            assert!(
                endpoint
                    .path_params
                    .iter()
                    .chain(endpoint.params)
                    .any(|p| p.name == param),
                "{call}"
            );
        }
    }
    assert!(skill.contains("https://dispatch.example.com/api/v1/mcp"));
}
