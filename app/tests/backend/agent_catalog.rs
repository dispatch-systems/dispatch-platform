//! The agents' catalog of every registered feature's endpoints and metrics.
use dispatch_core::mcp::data::catalog::*;
use serde_json::json;

#[test]
fn source_guidance_is_owned_by_installed_features() {
    crate::install();
    let instructions = dispatch_core::mcp::server::instructions();
    assert_eq!(
        instructions.contains("Daily Performance defaults to yesterday"),
        cfg!(feature = "daily_performance")
    );
    assert_eq!(
        instructions.contains("weekly_scorecard defaults to the latest week"),
        cfg!(feature = "weekly_scorecard")
    );
    for (guidance, installed) in [
        ("Routes default to yesterday", cfg!(feature = "routes")),
        (
            "Timecards default to yesterday for everyone",
            cfg!(feature = "timecard"),
        ),
        (
            "DVIC contains short exceptions only",
            cfg!(feature = "dvic"),
        ),
    ] {
        assert_eq!(instructions.contains(guidance), installed);
        assert!(!dispatch_core::mcp::server::INSTRUCTIONS.contains(guidance));
    }
    assert!(!dispatch_core::mcp::server::INSTRUCTIONS.contains("daily_performance"));
    assert!(!dispatch_core::mcp::server::INSTRUCTIONS.contains("weekly_scorecard"));
}

#[test]
fn every_endpoint_and_metric_is_listed_once() {
    crate::install();
    let mut ids: Vec<&str> = ENDPOINTS.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ENDPOINTS.len());
    let mut names: Vec<&str> = METRICS.iter().map(|m| m.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), METRICS.len());
    let mut tools: Vec<&str> = ENDPOINTS.iter().map(|e| e.tool).collect();
    tools.sort_unstable();
    tools.dedup();
    assert_eq!(tools.len(), ENDPOINTS.len());
    for endpoint in ENDPOINTS.iter() {
        assert_eq!(
            output(endpoint)["type"],
            "object",
            "{} must publish an MCP object output schema",
            endpoint.id
        );
        assert!(endpoint.path.starts_with("/api/v1/"), "{}", endpoint.path);
        // Names every model accepts: lower snake_case, well under 64 characters.
        assert!(
            endpoint.tool.len() <= 32
                && endpoint
                    .tool
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}",
            endpoint.tool
        );
        for param in endpoint.path_params {
            assert!(endpoint.path.contains(&format!("{{{}}}", param.name)));
        }
    }
}
#[cfg(feature = "dvic")]
#[test]
fn requests_take_only_their_own_parameters() {
    crate::install();
    assert!(
        check(
            "team",
            &json!({"period":"last week","per":"day","limit":"10"})
        )
        .is_ok()
    );
    let unknown = check("team", &json!({"week":"39"})).unwrap_err();
    assert_eq!(unknown.code, "unknown_parameter");
    assert!(unknown.choices.contains(&"period".to_owned()));
    assert_eq!(
        check("team", &json!({"per":"month"})).unwrap_err().code,
        "invalid_parameter"
    );
    assert_eq!(
        check("team", &json!({"limit":"0"})).unwrap_err().code,
        "invalid_parameter"
    );
    assert_eq!(
        check("dvic", &json!({"short":"maybe"})).unwrap_err().code,
        "invalid_parameter"
    );
    assert!(check("drivers", &json!({"q":"é".repeat(200)})).is_ok());
    assert_eq!(
        check("drivers", &json!({"q":"é".repeat(201)}))
            .unwrap_err()
            .code,
        "invalid_parameter"
    );
}
#[test]
fn the_openapi_document_lists_every_endpoint() {
    crate::install();
    let document = openapi("https://dispatch.example.com");
    assert_eq!(
        document["paths"].as_object().unwrap().len(),
        ENDPOINTS.len()
    );
    let team = &document["paths"]["/api/v1/team"]["get"];
    assert_eq!(team["operationId"], "team");
    assert!(
        team["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "metrics")
    );
}

#[cfg(all(feature = "daily_performance", feature = "weekly_scorecard"))]
#[test]
fn focused_tools_have_one_public_identity_and_select_source_before_parameter_checks() {
    crate::install();
    for id in ["feedback", "returns", "safety"] {
        assert_eq!(
            ENDPOINTS
                .iter()
                .filter(|endpoint| endpoint.id == id)
                .count(),
            1
        );
        let primary = select(endpoint(id), &json!({})).unwrap();
        assert!(
            endpoint(id)
                .description
                .contains("Select source: weekly_scorecard or daily_performance.")
        );
        assert_eq!(primary.area.unwrap().source().as_str(), "weekly_scorecard");
        let daily = select(endpoint(id), &json!({"source":"daily_performance"})).unwrap();
        assert_eq!(daily.area.unwrap().source().as_str(), "daily_performance");
        assert_eq!(primary.path, daily.path);
        assert_eq!(primary.tool, daily.tool);
        assert_eq!(output(endpoint(id))["anyOf"].as_array().unwrap().len(), 2);
        assert_eq!(
            select(endpoint(id), &json!({"source":"scorecard"}))
                .unwrap_err()
                .code,
            "unknown_source"
        );
        assert_eq!(
            select(endpoint(id), &json!({"source":["daily_performance"]}))
                .unwrap_err()
                .code,
            "invalid_parameter"
        );
    }
    assert!(
        check(
            "feedback",
            &json!({"source":"daily_performance","fields":"negative_response_cnt"})
        )
        .is_ok()
    );
    assert_eq!(
        check(
            "feedback",
            &json!({"source":"weekly_scorecard","fields":"negative_response_cnt"})
        )
        .unwrap_err()
        .code,
        "unknown_parameter"
    );
    assert_eq!(
        check(
            "returns",
            &json!({"source":"daily_performance","dataset":"driver_quality"})
        )
        .unwrap_err()
        .code,
        "unknown_parameter"
    );
}
