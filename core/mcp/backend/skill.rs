//! The Dispatch Agent Skill: one `SKILL.md`, in the open Agent Skills format that Claude
//! Code, Codex, Hermes and others load. It is written from the catalog, as the tools and the
//! OpenAPI document are, so it always describes the API this server answers.
use super::{
    data::catalog::{ENDPOINTS, Endpoint, GLOSSARY, METRICS},
    mcp::INSTRUCTIONS,
};

/// Questions people ask, with the tool and arguments that answer them.
const EXAMPLES: &[(&str, &str, &str)] = &[
    (
        "How many packages did Daniel deliver last week?",
        r#"packages(driver: "Daniel", period: "last week", outcome: "delivered")"#,
        "/api/v1/packages?driver=Daniel&period=last%20week&outcome=delivered",
    ),
    (
        "Did Daniel return any packages last night?",
        r#"packages(driver: "Daniel", date: "last night", outcome: "returned", group_by: "reason")"#,
        "/api/v1/packages?driver=Daniel&date=last%20night&outcome=returned&group_by=reason",
    ),
    (
        "How many business-closed packages did we have last night?",
        r#"packages(date: "last night", reason: "business_closed", group_by: "driver")"#,
        "/api/v1/packages?date=last%20night&reason=business_closed&group_by=driver",
    ),
    (
        "Which drivers were short on their DVIC?",
        r#"dvic_inspections(short: true)"#,
        "/api/v1/dvic?short=true",
    ),
    (
        "Which addresses have given us repeated negative feedback?",
        r#"customer_feedback(group_by: "address", min_count: 2)"#,
        "/api/v1/feedback?group_by=address&min_count=2",
    ),
    (
        "Which drivers didn't do contact compliance last week?",
        r#"returns(contact: "missed", period: "last week", group_by: "driver")"#,
        "/api/v1/returns?contact=missed&period=last%20week&group_by=driver",
    ),
    (
        "Did Daniel get any Netradyne infractions last week?",
        r#"safety_events(driver: "Daniel", period: "last week")"#,
        "/api/v1/safety?driver=Daniel&period=last%20week",
    ),
    (
        "Who scored lowest on last week's scorecard?",
        r#"scorecard(week: "last week")"#,
        "/api/v1/scorecard?week=last%20week",
    ),
    (
        "Who had the most stops yesterday?",
        r#"team_table(metrics: "stops_completed", date: "yesterday")"#,
        "/api/v1/team?metrics=stops_completed&date=yesterday",
    ),
    (
        "How did Daniel do last week?",
        r#"driver_report(driver: "Daniel", period: "last week")"#,
        "/api/v1/drivers/Daniel?period=last%20week",
    ),
];

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
         vehicle inspections, for one driver or the whole team, on any day or period. Use \
         when asked how a driver did, who led or trailed on a number, what happened on a \
         route or to a package, or about hours, lunches or inspections.\n\
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
         curl -fsS -H \"Authorization: Bearer $DISPATCH_KEY\" \"{origin}/api/v1/whoami\"\n\
         ```\n\n\
         MCP: `{origin}/api/v1/mcp`. OpenAPI: `{origin}/api/v1/openapi.json`.\n\n\
         ## Questions and the calls that answer them\n\n\
         | Question | Tool | REST |\n| --- | --- | --- |\n"
    ));
    for (question, tool, path) in EXAMPLES {
        out.push_str(&format!("| {question} | `{tool}` | `GET {path}` |\n"));
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_skill_names_every_tool_and_metric() {
        let skill = super::skill("https://dispatch.example.com");
        assert!(skill.starts_with("---\nname: dispatch\ndescription: "));
        let description = skill.lines().nth(2).unwrap();
        assert!(description.len() <= 1024 + "description: ".len());
        for endpoint in super::ENDPOINTS.iter() {
            assert!(
                skill.contains(&format!("`{}`", endpoint.tool)),
                "{}",
                endpoint.tool
            );
        }
        for metric in super::METRICS.iter() {
            assert!(
                skill.contains(&format!("`{}`", metric.name)),
                "{}",
                metric.name
            );
        }
        // Every example calls a tool that exists, with parameters it takes.
        for (_, call, _) in super::EXAMPLES {
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
}
