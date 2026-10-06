//! Agent-only views assembled from feature-owned adapters. No frontend or storage joins.
use super::{
    Answer, Failure, Refusal,
    access::{self, Access, Read},
    catalog::{self, Answerer, Endpoint, Kind, Param},
    schema,
    scope::param,
    shape,
};
use crate::{
    State,
    db::Store,
    manifest::registry,
    mcp::{Caller, api::types::AgentArea},
};
use serde_json::{Value, json};

pub struct Adapter {
    pub endpoint: &'static str,
    pub view: &'static str,
    pub area: AgentArea,
    pub answer: Answerer,
}
pub const PARAMS: &[Param] = &[
    Param {
        name: "view",
        kind: Kind::Choice(&["operational", "posted_scorecard", "compare"]),
        description: "v2: operational (yesterday); posted_scorecard or compare (last week).",
    },
    catalog::DETAIL,
    Param {
        name: "operational_cursor",
        kind: Kind::Text,
        description: "Compare: daily detail next_cursor.",
    },
    Param {
        name: "posted_cursor",
        kind: Kind::Text,
        description: "Compare: weekly detail next_cursor.",
    },
    Param {
        name: "operational_groups_cursor",
        kind: Kind::Text,
        description: "Compare: daily groups next_cursor.",
    },
    Param {
        name: "posted_groups_cursor",
        kind: Kind::Text,
        description: "Compare: weekly groups next_cursor.",
    },
];
pub fn adapters(id: &str) -> impl Iterator<Item = &'static Adapter> {
    registry()
        .features
        .iter()
        .flat_map(|f| f.mcp.performance)
        .filter(move |a| a.endpoint == id)
}
pub fn available(id: &str) -> bool {
    adapters(id).next().is_some()
}
fn adapter(id: &str, view: &str) -> Result<&'static Adapter, Refusal> {
    adapters(id).find(|a| a.view == view).ok_or_else(|| {
        Refusal::new(
            400,
            "unsupported_view",
            "This view's feature is not installed.",
        )
    })
}
pub fn check(endpoint: &Endpoint, query: &Value) -> Result<(), Refusal> {
    for (key, value) in query.as_object().into_iter().flatten() {
        if let Some(p) = PARAMS.iter().find(|p| p.name == key) {
            let text = value.as_str().unwrap_or("").trim();
            let valid = match p.kind {
                Kind::Choice(choices) => choices.contains(&text),
                _ => value.is_string() && text.len() <= 200,
            };
            if !valid {
                return Err(Refusal::new(
                    400,
                    "invalid_parameter",
                    format!("Invalid {key}."),
                ));
            }
        }
    }
    let view = param(query, "view");
    let views: &[&str] = if view == "compare" {
        &["operational", "posted_scorecard"]
    } else {
        &[view]
    };
    if view != "compare" && PARAMS[2..].iter().any(|p| query.get(p.name).is_some()) {
        return Err(Refusal::new(
            400,
            "invalid_parameter",
            "Source cursors belong to compare.",
        ));
    }
    if view == "compare"
        && ["source", "cursor", "groups_cursor"]
            .iter()
            .any(|p| query.get(*p).is_some())
    {
        return Err(Refusal::new(
            400,
            "invalid_parameter",
            "Compare uses source-specific cursors and no source selector.",
        ));
    }
    for view in views {
        let selected = adapter(endpoint.id, view)?;
        if query
            .get("source")
            .is_some_and(|s| s.as_str() != Some(selected.area.source().as_str()))
        {
            return Err(Refusal::new(
                400,
                "conflicting_source",
                "The source conflicts with the requested view.",
            ));
        }
        catalog::check(endpoint.id, &source_query(query, selected))?;
    }
    Ok(())
}
fn source_query(query: &Value, selected: &Adapter) -> Value {
    let full = param(query, "detail") == "full";
    let mut query = query.clone();
    let view = param(&query, "view").to_owned();
    let object = query.as_object_mut().expect("query object");
    let prefix = if selected.view == "operational" {
        "operational"
    } else {
        "posted"
    };
    for (target, suffix) in [("cursor", "cursor"), ("groups_cursor", "groups_cursor")] {
        if let Some(value) = object.get(&format!("{prefix}_{suffix}")).cloned() {
            object.insert(target.into(), value);
        }
    }
    for p in PARAMS {
        object.remove(p.name);
    }
    if full {
        query["list"] = json!("true");
    }
    if param(&query, "contact") == "all" {
        query.as_object_mut().expect("query").remove("contact");
    }
    if catalog::has_variants(selected.endpoint) {
        query["source"] = json!(selected.area.source().as_str());
    } else {
        query.as_object_mut().expect("query").remove("source");
    }
    if ["period", "date", "from", "to"]
        .iter()
        .all(|key| query.get(*key).is_none())
    {
        query["period"] = json!(if view == "operational" {
            "yesterday"
        } else {
            "last week"
        });
    }
    query
}

pub fn ask(
    endpoint: &Endpoint,
    db: &Store,
    state: &State,
    caller: &Caller,
    named: &str,
    query: &Value,
) -> Answer {
    check(endpoint, query)?;
    let view = param(query, "view");
    let access = Access::of(db, caller, query)?;
    let views: &[&str] = if view == "compare" {
        &["operational", "posted_scorecard"]
    } else {
        &[view]
    };
    // Gate every requested source before reading any of them. Never silently fall back.
    let selected = views
        .iter()
        .map(|view| {
            let a = adapter(endpoint.id, view)?;
            Ok((a, access.check(a.area)?))
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let mut answers = serde_json::Map::new();
    for (a, read) in selected {
        let scoped = source_query(query, a);
        let result = (a.answer)(db, state, caller, named, &scoped);
        let mut answer = match result {
            Ok(answer) => answer,
            Err(Failure::Refused(refusal)) if view == "compare" && refusal.status == 404 => {
                let (_, body) = refusal.body();
                json!({"counts":null,"coverage":{"status":"unavailable"},"unavailable":body})
            }
            Err(error) => return Err(error),
        };
        answer["contract_version"] = json!(2);
        answer["view"] = json!(a.view);
        answer["source"] = json!(a.area.source().as_str());
        if read == Read::Bypassed {
            access::bypassed(&mut answer, a.area.source());
        }
        if view != "compare" {
            shape::check_budget(&answer)?;
            return Ok(answer);
        }
        answers.insert(a.view.into(), answer);
    }
    let mut answer = json!({"contract_version":2,"view":"compare","sources":answers,
        "note":"Sources are independent and are not additive. Scorecard impact belongs to posted_scorecard. \
            Units and coverage may differ."});
    // Both sources keep their own cursors; trim only pageable tables, preserving full totals.
    for key in ["operational", "posted_scorecard"] {
        for table in ["groups", "list"] {
            let source = &mut answer["sources"][key];
            if let Some(value) = source.get(table).cloned() {
                let rows = value["rows"].as_array().cloned().unwrap_or_default();
                let cursor_name = if key == "operational" {
                    if table == "list" {
                        "operational_cursor"
                    } else {
                        "operational_groups_cursor"
                    }
                } else if table == "list" {
                    "posted_cursor"
                } else {
                    "posted_groups_cursor"
                };
                let start = shape::offset_named(query, cursor_name)?;
                let total = value["page"]["total"]
                    .as_u64()
                    .map_or(start + rows.len(), |n| n as usize);
                let columns = value["columns"].as_array().cloned().unwrap_or_default();
                // Preserve owner-declared column names, without extending their lifetime.
                let mut shown = rows.len();
                while source[table].to_string().len() > shape::BUDGET / 4 - 512 && shown > 0 {
                    shown -= 1;
                    source[table] = json!({"columns":columns,"rows":&rows[..shown],
                        "page":{"returned":shown,"total":total,"next_cursor":(start+shown).to_string()}});
                }
                if shown == 0 && total > start {
                    return Err(Refusal::new(
                        400,
                        "answer_too_large",
                        "Ask for a smaller comparison or summary.",
                    )
                    .into());
                }
                if source[table]["page"].get("next_cursor").is_some() {
                    source[table]["page"]["cursor_parameter"] = json!(cursor_name);
                    source[table]["note"] = json!(format!(
                        "Follow next_cursor using {cursor_name} with the same filters."
                    ));
                }
            }
        }
    }
    let bypassed: Vec<Value> = answer["sources"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .flat_map(|source| source["bypassed"].as_array().into_iter().flatten().cloned())
        .collect();
    if !bypassed.is_empty() {
        answer["bypassed"] = json!(bypassed);
    }
    shape::check_budget(&answer)?;
    Ok(answer)
}
pub fn output() -> Value {
    let mut output = schema::answer(
        &[
            ("contract_version", json!({"const":2})),
            ("view", schema::text()),
            ("source", schema::text()),
            ("counts", schema::nullable(schema::object(&[], &[]))),
            ("coverage", schema::object(&[], &[])),
            ("sources", schema::object(&[], &[])),
            ("list", schema::table(&[])),
            ("groups", schema::table(&[])),
        ],
        &["contract_version", "view"],
    );
    output["required"] = json!(["contract_version", "view"]);
    output
}
