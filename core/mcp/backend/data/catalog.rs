//! Everything an agent can ask, written once: each endpoint with its parameters, and each
//! metric with what it means. Requests are checked against it, the OpenAPI document is
//! built from it, and later the MCP tools are too, so none of them can drift apart. Core
//! declares the endpoints that join every feature's facts; each feature declares its own
//! endpoints, metrics and terms in its manifest's `mcp`.
use super::{Answer, Refusal};
use crate::{
    State,
    db::Store,
    manifest::registry,
    mcp::{Caller, api::types::AgentArea},
};
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
    /// The successful response, declared by the endpoint's owner for both surfaces.
    pub output: fn() -> Value,
}
/// Another feature's source for an existing public endpoint. Identity stays with its owner.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub endpoint: &'static str,
    pub area: AgentArea,
    pub params: &'static [Param],
    pub answer: Answerer,
    pub output: fn() -> Value,
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
pub const GROUPS_CURSOR: Param = Param {
    name: "groups_cursor",
    kind: Kind::Text,
    description: "The groups table's next_cursor; cursor separately pages the detail list.",
};
pub const RECENT_PERIOD: Param = Param {
    description: "Days in the DSP's time: last week, last 14 days or 2026-W39. Defaults to yesterday.",
    ..PERIOD
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
        output: super::schema::whoami,
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
        description: "Use when asked how current the data is: which collected sources the DSP has on, \
            which this key reads there, and when each last collected.",
        path_params: &[],
        params: &[DSP],
        order: 20,
        output: super::schema::status,
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
        output: super::schema::metrics,
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
        output: super::schema::drivers,
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
        params: &[
            DSP,
            PERIOD,
            DATE,
            FROM,
            TO,
            Param {
                name: "metrics",
                kind: Kind::Text,
                description: "Optional comma-separated list_metrics names; reads only those sources. Omit for all daily facts.",
            },
            DETAIL,
            LIMIT,
            CURSOR,
        ],
        order: 60,
        output: super::schema::driver,
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
        output: super::schema::team,
        answer: |db, state, caller, _, query| super::team(db, state, caller, query),
    },
];

/// Every endpoint, core's and each feature's, in their order.
fn declared() -> impl Iterator<Item = &'static Endpoint> {
    CORE.iter().chain(
        registry()
            .features
            .iter()
            .flat_map(|feature| feature.mcp.endpoints),
    )
}
// Resolve alternate source metadata once. Removing the endpoint owner also removes its
// focused tools; alternate owners' generic tools remain independently available.
static SOURCES: LazyLock<Vec<Endpoint>> = LazyLock::new(|| {
    registry()
        .features
        .iter()
        .flat_map(|feature| feature.mcp.variants)
        .filter_map(|variant| {
            declared()
                .find(|endpoint| endpoint.id == variant.endpoint)
                .map(|endpoint| Endpoint {
                    area: Some(variant.area),
                    params: variant.params,
                    answer: variant.answer,
                    output: variant.output,
                    ..*endpoint
                })
        })
        .collect()
});
pub static ENDPOINTS: LazyLock<Vec<&'static Endpoint>> = LazyLock::new(|| {
    let mut all: Vec<&'static Endpoint> = declared().collect();
    for endpoint in &mut all {
        let sources = alternatives(endpoint.id);
        if has_variants(endpoint.id) {
            let mut params = endpoint.params.to_vec();
            for alternative in &sources {
                for param in alternative.params {
                    if let Some(existing) = params
                        .iter_mut()
                        .find(|existing| existing.name == param.name)
                    {
                        existing.kind = union_kind(existing.kind, param.kind);
                    } else {
                        params.push(*param);
                    }
                }
            }
            let choices: Vec<&'static str> = sources
                .iter()
                .filter_map(|source| source.area.map(|area| area.source().as_str()))
                .collect();
            params.push(Param {name:"source",kind:Kind::Choice(Box::leak(choices.into_boxed_slice())),
                description:"Choose the data source explicitly. Omit for the first listed source; sources never mix or fall back."});
            *endpoint = Box::leak(Box::new(Endpoint {
                params: Box::leak(params.into_boxed_slice()),
                ..**endpoint
            }));
        }
        if super::performance::available(endpoint.id) {
            let mut params = endpoint.params.to_vec();
            for param in super::performance::params(endpoint.id) {
                if !params.iter().any(|existing| existing.name == param.name) {
                    params.push(param);
                }
            }
            *endpoint = Box::leak(Box::new(Endpoint {
                params: Box::leak(params.into_boxed_slice()),
                ..**endpoint
            }));
        }
    }
    all.sort_by_key(|endpoint| endpoint.order);
    all
});
fn union_kind(first: Kind, second: Kind) -> Kind {
    match (first, second) {
        (Kind::Choice(a), Kind::Choice(b)) => {
            let mut choices = a.to_vec();
            for choice in b {
                if !choices.contains(choice) {
                    choices.push(choice);
                }
            }
            Kind::Choice(Box::leak(choices.into_boxed_slice()))
        }
        (Kind::Text, _) | (_, Kind::Text) => Kind::Text,
        _ => first,
    }
}
/// Actual owner declarations; merged discovery entries never decide access.
pub fn alternatives(id: &str) -> Vec<&'static Endpoint> {
    declared()
        .chain(SOURCES.iter())
        .filter(|endpoint| endpoint.id == id)
        .collect()
}
pub fn has_variants(id: &str) -> bool {
    SOURCES.iter().any(|source| source.id == id)
}
pub fn select(endpoint: &Endpoint, query: &Value) -> Result<&'static Endpoint, Refusal> {
    let sources = alternatives(endpoint.id);
    if query
        .get("source")
        .is_some_and(|source| source.as_str().is_none_or(|value| value.trim().is_empty()))
    {
        return Err(Refusal::new(
            400,
            "invalid_parameter",
            "source must name a declared source.",
        ));
    }
    let wanted = super::scope::param(query, "source");
    if wanted.is_empty() {
        return Ok(*sources.first().expect("declared endpoint"));
    }
    sources
        .iter()
        .copied()
        .find(|source| {
            source
                .area
                .is_some_and(|area| area.source().as_str() == wanted)
        })
        .ok_or_else(|| {
            Refusal::new(
                400,
                "unknown_source",
                "Choose a declared source for this tool.",
            )
            .choices(
                sources
                    .iter()
                    .filter_map(|source| source.area.map(|area| area.source().as_str().into()))
                    .collect(),
            )
        })
}
pub fn output(endpoint: &Endpoint) -> Value {
    let sources = alternatives(endpoint.id);
    let mut schemas: Vec<Value> = sources
        .iter()
        .map(|source| {
            let mut schema = (source.output)();
            if has_variants(endpoint.id)
                && let Some(area) = source.area
            {
                schema["properties"]["source"] =
                    json!({"type":"string","const":area.source().as_str()});
                if let Some(required) = schema["required"].as_array_mut()
                    && !required.iter().any(|field| field == "source")
                {
                    required.push(json!("source"));
                }
            }
            schema
        })
        .collect();
    if super::performance::available(endpoint.id) {
        schemas.push(super::performance::output());
    }
    if schemas.len() == 1 {
        schemas.into_iter().next().expect("schema")
    } else {
        // Share identical properties across source contracts instead of repeating them
        // in every discovery branch. Branch requirements remain source-specific.
        let mut shared = Map::new();
        let candidates = schemas[0]["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (name, value) in candidates {
            if schemas
                .iter()
                .all(|schema| schema["properties"].get(&name) == Some(&value))
            {
                for schema in &mut schemas {
                    schema["properties"]
                        .as_object_mut()
                        .expect("properties")
                        .remove(&name);
                }
                shared.insert(name, value);
            }
        }
        json!({"type":"object","properties":shared,"anyOf":schemas})
    }
}

pub fn endpoint(id: &str) -> &'static Endpoint {
    ENDPOINTS
        .iter()
        .copied()
        .find(|e| e.id == id)
        .expect("a listed endpoint")
}

/// Refuses a parameter the endpoint does not take, or a value of the wrong kind.
pub fn check(id: &str, query: &Value) -> Result<(), Refusal> {
    let discovery = endpoint(id);
    if query.get("view").is_some() && super::performance::available(id) {
        return super::performance::check(discovery, query);
    }
    let endpoint = select(discovery, query)?;
    let names = || endpoint.params.iter().map(|p| p.name.to_owned()).collect();
    for (name, value) in query.as_object().into_iter().flatten() {
        if name == "source" && discovery.params.iter().any(|param| param.name == "source") {
            continue;
        }
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
        meaning: "Amazon's ID for a driver across its data sources.",
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
                    "200": response("The answer.", output(endpoint)),
                    "400": response("Something unclear; message says what and choices what it could mean.", super::schema::error()),
                    "401": response("No key, or a key that is revoked, expired or not for this Dispatch.", super::schema::error()),
                    "403": response("Not for this key here: not_allowed when the key may not read it at the DSP, \
                        source_off when the DSP has the feature switched off.", super::schema::error()),
                    "404": response("Nothing by that name, or data not collected or posted yet.", super::schema::error()),
                    "422": response("The query needs too much database work; narrow the days or filters.", super::schema::error()),
                    "429": retry_response("Too many calls this minute; wait for Retry-After seconds."),
                    "500": response("Dispatch could not answer.", super::schema::error()),
                    "503": retry_response("Dispatch is temporarily busy or unavailable.")
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

fn response(description: &str, schema: Value) -> Value {
    json!({"description":description,"content":{"application/json":{"schema":schema}}})
}
fn retry_response(description: &str) -> Value {
    let mut response = response(description, super::schema::error());
    response["headers"] = json!({"Retry-After":{"description":"Seconds to wait before retrying.",
        "schema":{"type":"integer","minimum":1}}});
    response
}
