//! Everything an agent can ask, written once: each endpoint with its parameters, and each
//! metric with what it means. Requests are checked against it, the OpenAPI document is
//! built from it, and later the MCP tools are too, so none of them can drift apart. Core
//! declares the endpoints that join every feature's facts; each feature declares its own
//! endpoints, metrics and terms in its manifest's `mcp`.
use super::{Answer, Refusal};
use crate::{State, agents::Caller, contracts::AgentArea, db::Store, manifest::registry};
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

/// What a parameter holds.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Text,
    Integer(i64, i64),
    Boolean,
    Choice(&'static [&'static str]),
}
#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
    pub description: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub struct Endpoint {
    pub id: &'static str,
    /// The MCP tool's name: snake_case, as every model and harness accepts.
    pub tool: &'static str,
    /// The one kind of data it reads, refused at a DSP where the key or app may not read it
    /// before the endpoint is asked. `None` for those that read none, or several and say
    /// which they could not read.
    pub area: Option<AgentArea>,
    pub path: &'static str,
    pub summary: &'static str,
    pub description: &'static str,
    /// The parts of the path an agent fills in, such as `{driver}`.
    pub path_params: &'static [Param],
    pub params: &'static [Param],
    /// Its place in the one order the catalog lists endpoints in, and so the OpenAPI
    /// document, the MCP tools and the skill.
    pub order: u16,
    pub answer: Answerer,
}
/// Answers one endpoint, given what fills its path parameter (empty when it has none) and
/// its query, once the key or app is known to read what it reads.
pub type Answerer = fn(&Store, &State, &Caller, &str, &Value) -> Answer;

pub const DSP: Param = Param {
    name: "dsp",
    kind: Kind::Text,
    description: "The DSP, by name. Leave it out when the key reaches one DSP.",
};
pub const PERIOD: Param = Param {
    name: "period",
    kind: Kind::Text,
    description: "The days as the user said them, read in the DSP's own time: yesterday, \
        last night, last week, this month, last 14 days, 2026-09-28, 2026-09-01..2026-09-30 \
        or 2026-W39. Weeks run Sunday to Saturday. Leave out for the last 30 days.",
};
pub const DATE: Param = Param {
    name: "date",
    kind: Kind::Text,
    description: "One day: today, yesterday, last night or 2026-09-28.",
};
pub const DAY: Param = Param {
    name: "date",
    kind: Kind::Text,
    description: "The day: yesterday, last night, today or 2026-09-28. Leave out for yesterday.",
};
pub const FROM: Param = Param {
    name: "from",
    kind: Kind::Text,
    description: "The first day, as 2026-09-01, with `to`; instead of `period`.",
};
pub const TO: Param = Param {
    name: "to",
    kind: Kind::Text,
    description: "The last day, as 2026-09-30, with `from`.",
};
pub const DRIVER: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: "A driver as the user named them: a name or part of one, a Driver Match \
        code, a Paycom employee code or an Amazon transporter ID.",
};
pub const LIMIT: Param = Param {
    name: "limit",
    kind: Kind::Integer(1, 500),
    description: "The most rows to return, 1 to 500.",
};
pub const CURSOR: Param = Param {
    name: "cursor",
    kind: Kind::Text,
    description: "The next_cursor an earlier answer gave, for its next page.",
};
pub const DETAIL: Param = Param {
    name: "detail",
    kind: Kind::Choice(&["summary", "full"]),
    description: "summary (the default) or full, only when the user wants every row.",
};

pub const DRIVER_PATH: Param = Param {
    name: "driver",
    kind: Kind::Text,
    description: DRIVER.description,
};

/// The endpoints core answers: who is asking, how fresh each source is, what the metrics
/// mean, and the questions every feature's facts are joined for.
pub(crate) const CORE: &[Endpoint] = &[
    Endpoint {
        id: "whoami",
        tool: "whoami",
        area: None,
        path: "/api/v1/whoami",
        summary: "Who this key belongs to",
        description: "Use when you need the DSPs this key reaches, each DSP's date today or \
            what the key may do and read there. Questions about days need no call to this \
            first: every answer says which days it read.",
        path_params: &[],
        params: &[],
        order: 10,
        answer: |db, _, caller, _, query| {
            check("whoami", query)?;
            Ok(json!(db.agent_whoami(caller)?))
        },
    },
    Endpoint {
        id: "status",
        tool: "data_status",
        area: None,
        path: "/api/v1/status",
        summary: "How fresh each source is",
        description: "Use when asked how current the data is: which sources the DSP has on \
            (Paycom timecards, Cortex meal breaks, routes, DVIC, the scorecard), which this key \
            reads there, and when each last collected.",
        path_params: &[],
        params: &[DSP],
        order: 20,
        answer: |db, _, caller, _, query| super::status(db, caller, query),
    },
    Endpoint {
        id: "metrics",
        tool: "list_metrics",
        area: None,
        path: "/api/v1/metrics",
        summary: "Every metric and term",
        description: "Use when unsure what a team_table metric or an Amazon term means: each \
            metric's source, unit and meaning, and a glossary.",
        path_params: &[],
        params: &[],
        order: 30,
        answer: |_, _, _, _, query| super::metrics(query),
    },
    Endpoint {
        id: "drivers",
        tool: "find_drivers",
        area: None,
        path: "/api/v1/drivers",
        summary: "Find drivers",
        description: "Use to look a driver up or list the DSP's people: name, Driver Match \
            code and matching status. Other tools already accept a driver's name, so look up \
            only when asked to. Provider IDs are included only when explicitly requested.",
        path_params: &[],
        params: &[
            DSP,
            Param {
                name: "q",
                kind: Kind::Text,
                description: "Part of a name, a code or an ID.",
            },
            Param {
                name: "include_ids",
                kind: Kind::Boolean,
                description: "Include Paycom employee codes and Amazon transporter IDs only \
                    when the user explicitly needs provider identifiers.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 40,
        answer: |db, state, caller, _, query| super::drivers(db, state, caller, query),
    },
    Endpoint {
        id: "driver",
        tool: "driver_report",
        area: None,
        path: "/api/v1/drivers/{driver}",
        summary: "One driver's days",
        description: "Use for how one driver did over some days, and their averages: one \
            line per day with their route, stops, packages delivered and undeliverable, hours, \
            clock in and out, meal break and inspections, plus totals.",
        path_params: &[DRIVER_PATH],
        params: &[DSP, PERIOD, DATE, FROM, TO, DETAIL, LIMIT, CURSOR],
        order: 60,
        answer: |db, state, caller, named, query| super::driver(db, state, caller, named, query),
    },
    Endpoint {
        id: "team",
        tool: "team_table",
        area: None,
        path: "/api/v1/team",
        summary: "Compare drivers",
        description: "Use to rank or compare drivers on chosen metrics: one row per driver, \
            or per driver per day, with the team's total for each metric.",
        path_params: &[],
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            Param {
                name: "metrics",
                kind: Kind::Text,
                description: "Comma-separated metric names; stops, packages and hours when \
                    left out.",
            },
            Param {
                name: "per",
                kind: Kind::Choice(&["driver", "day"]),
                description: "driver (totals, the default) or day.",
            },
            Param {
                name: "sort",
                kind: Kind::Text,
                description: "One of the metrics asked for, or name.",
            },
            Param {
                name: "order",
                kind: Kind::Choice(&["highest", "lowest"]),
                description: "highest first (the default) or lowest first, as for who did the \
                    fewest.",
            },
            LIMIT,
            CURSOR,
        ],
        order: 70,
        answer: |db, state, caller, _, query| super::team(db, state, caller, query),
    },
];

/// Every endpoint, core's and each feature's, in their order.
pub static ENDPOINTS: LazyLock<Vec<&'static Endpoint>> = LazyLock::new(|| {
    let features = registry().features.iter().flat_map(|f| f.mcp.endpoints);
    let mut all: Vec<&'static Endpoint> = CORE.iter().chain(features).collect();
    all.sort_by_key(|endpoint| endpoint.order);
    all
});

pub fn endpoint(id: &str) -> &'static Endpoint {
    ENDPOINTS
        .iter()
        .copied()
        .find(|e| e.id == id)
        .expect("a listed endpoint")
}

/// Refuses a parameter the endpoint does not take, or a value of the wrong kind.
pub fn check(id: &str, query: &Value) -> Result<(), Refusal> {
    let endpoint = endpoint(id);
    let names = || endpoint.params.iter().map(|p| p.name.to_owned()).collect();
    for (name, value) in query.as_object().into_iter().flatten() {
        let Some(param) = endpoint.params.iter().find(|p| p.name == name) else {
            return Err(Refusal::new(
                400,
                "unknown_parameter",
                format!("{} takes no parameter `{name}`.", endpoint.path),
            )
            .choices(names()));
        };
        let text = value.as_str().unwrap_or("").trim();
        let fits = match param.kind {
            Kind::Text => text.chars().count() <= 200,
            Kind::Integer(low, high) => {
                text.parse::<i64>().is_ok_and(|n| (low..=high).contains(&n))
            }
            Kind::Boolean => ["true", "false", "1", "0", "yes", "no"].contains(&text),
            Kind::Choice(choices) => choices.contains(&text),
        };
        if !fits {
            let expected = match param.kind {
                Kind::Text => "at most 200 characters".to_owned(),
                Kind::Integer(low, high) => format!("a whole number from {low} to {high}"),
                Kind::Boolean => "true or false".to_owned(),
                Kind::Choice(choices) => format!("one of {}", choices.join(", ")),
            };
            return Err(Refusal::new(
                400,
                "invalid_parameter",
                format!("`{name}` is {expected}."),
            ));
        }
    }
    Ok(())
}
/// A boolean parameter: true when given as true, 1 or yes.
pub fn flag(query: &Value, name: &str) -> bool {
    ["true", "1", "yes"].contains(&super::scope::param(query, name))
}

/// What a metric counts, the kind of data it comes from, and how a period adds it up.
#[derive(Clone, Copy, Debug)]
pub struct Metric {
    pub name: &'static str,
    pub area: AgentArea,
    pub unit: &'static str,
    /// `sum` and `count` add up over a period; `day` metrics exist only per day.
    pub total: &'static str,
    pub description: &'static str,
}
/// Every metric the features declare: those of each kind of data in the kinds' order, and
/// each feature's as it lists them.
pub static METRICS: LazyLock<Vec<&'static Metric>> = LazyLock::new(|| {
    let mut all: Vec<&'static Metric> = registry()
        .features
        .iter()
        .flat_map(|f| f.mcp.metrics)
        .collect();
    all.sort_by_key(|metric| metric.area.order());
    all
});
pub fn metric(name: &str) -> Option<&'static Metric> {
    METRICS.iter().copied().find(|m| m.name == name)
}

/// A word an answer uses, and what it means.
pub struct Term {
    pub term: &'static str,
    pub meaning: &'static str,
    /// Its place in the glossary.
    pub order: u16,
}

/// What core's answers say that an agent may not know: how sources know drivers, and
/// how periods are named.
pub(crate) const CORE_TERMS: &[Term] = &[
    Term {
        term: "transporter ID",
        meaning: "Amazon's ID for a driver, in routes, meal breaks, DVIC and the scorecard.",
        order: 20,
    },
    Term {
        term: "employee code",
        meaning: "Paycom's ID for an employee.",
        order: 30,
    },
    Term {
        term: "Amazon week",
        meaning: "Sunday to Saturday, named by the ISO week of its Saturday, as 2026-W39.",
        order: 40,
    },
];

/// The glossary: core's terms and each feature's, in their order.
pub static GLOSSARY: LazyLock<Vec<&'static Term>> = LazyLock::new(|| {
    let features = registry().features.iter().flat_map(|f| f.mcp.terms);
    let mut all: Vec<&'static Term> = CORE_TERMS.iter().chain(features).collect();
    all.sort_by_key(|term| term.order);
    all
});

/// The metrics and glossary, as `/api/v1/metrics` answers.
pub fn metrics() -> Value {
    json!({
        "metrics": METRICS.iter().map(|m| json!({
            "name": m.name, "source": m.area.as_str(), "unit": m.unit,
            "total": m.total, "description": m.description
        })).collect::<Vec<_>>(),
        "glossary": GLOSSARY.iter().map(|t| json!({"term": t.term, "meaning": t.meaning})).collect::<Vec<_>>(),
    })
}

fn schema(kind: Kind) -> Value {
    match kind {
        Kind::Text => json!({"type":"string","maxLength":200}),
        Kind::Integer(low, high) => json!({"type":"integer","minimum":low,"maximum":high}),
        Kind::Boolean => json!({"type":"boolean"}),
        Kind::Choice(choices) => json!({"type":"string","enum":choices}),
    }
}

/// The endpoint an MCP tool asks.
pub fn tool(name: &str) -> Option<&'static Endpoint> {
    ENDPOINTS.iter().copied().find(|e| e.tool == name)
}

/// An MCP tool's input: one flat object of strings, numbers, yes-or-no and fixed choices,
/// which every model's function calling accepts. The parts of the path are required.
pub fn input_schema(endpoint: &Endpoint) -> Map<String, Value> {
    let mut properties = Map::new();
    for param in endpoint.path_params.iter().chain(endpoint.params) {
        let mut property = schema(param.kind);
        let description = if param.name == "metrics" {
            let names: Vec<&str> = METRICS.iter().map(|m| m.name).collect();
            format!(
                "{} One or more of: {}.",
                param.description,
                names.join(", ")
            )
        } else {
            param.description.to_owned()
        };
        property["description"] = json!(description);
        properties.insert(param.name.to_owned(), property);
    }
    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    schema.insert("additionalProperties".into(), json!(false));
    if !endpoint.path_params.is_empty() {
        let required: Vec<&str> = endpoint.path_params.iter().map(|p| p.name).collect();
        schema.insert("required".into(), json!(required));
    }
    schema
}

/// The OpenAPI 3.1 document for the agent API, for tools that read one.
pub fn openapi(origin: &str) -> Value {
    let mut paths = Map::new();
    for endpoint in ENDPOINTS.iter() {
        let parameters: Vec<Value> = endpoint
            .path_params
            .iter()
            .map(|p| json!({"name":p.name,"in":"path","required":true,"description":p.description,"schema":schema(p.kind)}))
            .chain(endpoint.params.iter().map(|p| {
                json!({"name":p.name,"in":"query","required":false,"description":p.description,"schema":schema(p.kind)})
            }))
            .collect();
        paths.insert(
            endpoint.path.to_owned(),
            json!({"get": {
                "operationId": endpoint.id,
                "summary": endpoint.summary,
                "description": endpoint.description,
                "parameters": parameters,
                "responses": {
                    "200": {"description": "The answer.", "content": {"application/json": {"schema": {"type":"object"}}}},
                    "400": {"description": "Something unclear; `message` says what and `choices` what it could mean."},
                    "401": {"description": "No key, or a key that is revoked, expired or not for this Dispatch."},
                    "403": {"description": "Not for this key here: `not_allowed` when the key may not read it at \
                        the DSP, `source_off` when the DSP has the feature switched off."},
                    "404": {"description": "Nothing by that name."},
                    "429": {"description": "Too many calls this minute; wait for `Retry-After` seconds."}
                }
            }}),
        );
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Dispatch agent API",
            "version": "1",
            "description": "Read a DSP's drivers, routes, timecards, meal breaks, DVIC and \
                scorecard, as far as the key may read them there. Every answer names the DSP, \
                the days and the driver it understood, and which days each source has. An \
                answer read from a feature the DSP has switched off, by a key that bypasses \
                features, names it under `bypassed`. Times are the DSP's own."
        },
        "servers": [{"url": origin}],
        "security": [{"key": []}],
        "components": {"securitySchemes": {"key": {
            "type": "http",
            "scheme": "bearer",
            "description": "An agent key made on the Agents page, or an OAuth access token issued by Sign in with Dispatch."
        }}},
        "paths": paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
