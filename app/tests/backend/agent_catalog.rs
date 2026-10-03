//! The agents' catalog of every registered feature's endpoints and metrics.
use crate::agents::data::catalog::*;
use serde_json::json;

#[test]
fn every_endpoint_and_metric_is_listed_once() {
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
#[test]
fn requests_take_only_their_own_parameters() {
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
