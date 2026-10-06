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
    pub cursor_prefix: &'static str,
    pub default_period: &'static str,
    /// This adapter's default is the shared period when comparing all registered views.
    pub compare_default: bool,
    pub normalize: fn(&mut Value),
    pub area: AgentArea,
    pub answer: Answerer,
}
/// Built once with catalog discovery; feature-owned names become public parameters.
pub fn params(id: &str) -> Vec<Param> {
    let mut registered: Vec<_> = adapters(id).collect();
    registered.sort_by_key(|a| a.view);
    let mut choices: Vec<_> = registered.iter().map(|a| a.view).collect();
    if registered.len() > 1 {
        choices.push("compare");
    }
    let mut params = vec![
        Param {
            name: "view",
            kind: Kind::Choice(Box::leak(choices.into_boxed_slice())),
            description: "v2: choose a registered view or compare their independent answers.",
        },
        catalog::DETAIL,
    ];
    if registered.len() > 1 {
        for a in registered {
            for suffix in ["cursor", "groups_cursor"] {
                params.push(Param {
                    name: Box::leak(format!("{}_{suffix}", a.cursor_prefix).into_boxed_str()),
                    kind: Kind::Text,
                    description: "Compare: this view's next_cursor for the matching table.",
                });
            }
        }
    }
    params
}
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
pub fn is_cursor(id: &str, name: &str) -> bool {
    adapters(id).any(|a| {
        name.strip_prefix(a.cursor_prefix)
            .is_some_and(|suffix| matches!(suffix, "_cursor" | "_groups_cursor"))
    })
}
fn selected(id: &str, view: &str) -> Result<Vec<&'static Adapter>, Refusal> {
    let registered: Vec<_> = adapters(id)
        .filter(|a| view == "compare" || a.view == view)
        .collect();
    if registered.is_empty() || view == "compare" && registered.len() < 2 {
        return Err(Refusal::new(
            400,
            "unsupported_view",
            "The requested view is unavailable in this build.",
        ));
    }
    Ok(registered)
}
pub fn check(endpoint: &Endpoint, query: &Value) -> Result<(), Refusal> {
    for (key, value) in query.as_object().into_iter().flatten() {
        if let Some(p) = endpoint.params.iter().find(|p| {
            p.name == key
                && (p.name == "view" || p.name == "detail" || is_cursor(endpoint.id, p.name))
        }) {
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
    let selected = selected(endpoint.id, view)?;
    if view != "compare"
        && query
            .as_object()
            .into_iter()
            .flatten()
            .any(|(key, _)| is_cursor(endpoint.id, key))
    {
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
    for selected in selected {
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
        catalog::check(endpoint.id, &source_query(endpoint, query, selected))?;
    }
    Ok(())
}
fn source_query(endpoint: &Endpoint, query: &Value, selected: &Adapter) -> Value {
    let full = param(query, "detail") == "full";
    let mut query = query.clone();
    let view = param(&query, "view").to_owned();
    let object = query.as_object_mut().expect("query object");
    for (target, suffix) in [("cursor", "cursor"), ("groups_cursor", "groups_cursor")] {
        if let Some(value) = object
            .get(&format!("{}_{suffix}", selected.cursor_prefix))
            .cloned()
        {
            object.insert(target.into(), value);
        }
    }
    for p in endpoint.params {
        if p.name == "view" || p.name == "detail" || is_cursor(endpoint.id, p.name) {
            object.remove(p.name);
        }
    }
    if full {
        query["list"] = json!("true");
    }
    (selected.normalize)(&mut query);
    if catalog::has_variants(selected.endpoint) {
        query["source"] = json!(selected.area.source().as_str());
    } else {
        query.as_object_mut().expect("query").remove("source");
    }
    if ["period", "date", "from", "to"]
        .iter()
        .all(|key| query.get(*key).is_none())
    {
        query["period"] = json!(if view == "compare" {
            adapters(endpoint.id)
                .find(|a| a.compare_default)
                .or_else(|| adapters(endpoint.id).next())
                .expect("registered comparison view")
                .default_period
        } else {
            selected.default_period
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
    // Gate every requested source before reading any of them. Never silently fall back.
    let selected = selected(endpoint.id, view)?
        .into_iter()
        .map(|a| Ok((a, access.check(a.area)?)))
        .collect::<Result<Vec<_>, Refusal>>()?;
    let mut answers = serde_json::Map::new();
    for (a, read) in &selected {
        let scoped = source_query(endpoint, query, a);
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
        if *read == Read::Bypassed {
            access::bypassed(&mut answer, a.area.source());
        }
        if view != "compare" {
            fit(&mut answer, query)?;
            return Ok(answer);
        }
        answers.insert(a.view.into(), answer);
    }
    let mut answer = json!({"contract_version":2,"view":"compare","sources":answers,
        "note":"Sources are independent and are not additive. Units and coverage may differ."});
    // Each source keeps its own cursors; trim only pageable tables, preserving full totals.
    let table_budget = (shape::BUDGET / (selected.len() * 2)).saturating_sub(512);
    for (a, _) in selected {
        for table in ["groups", "list"] {
            let source = &mut answer["sources"][a.view];
            if let Some(value) = source.get(table).cloned() {
                let rows = value["rows"].as_array().cloned().unwrap_or_default();
                let suffix = if table == "list" {
                    "cursor"
                } else {
                    "groups_cursor"
                };
                let cursor_name = format!("{}_{suffix}", a.cursor_prefix);
                let start = shape::offset_named(query, &cursor_name)?;
                let total = value["page"]["total"]
                    .as_u64()
                    .map_or(start + rows.len(), |n| n as usize);
                let columns = value["columns"].as_array().cloned().unwrap_or_default();
                // Preserve owner-declared column names, without extending their lifetime.
                let mut shown = rows.len();
                while source[table].to_string().len() > table_budget && shown > 0 {
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
/// Owner pages precede v2 metadata. Refit those pages without changing their totals.
fn fit(answer: &mut Value, query: &Value) -> Result<(), Refusal> {
    for (key, cursor) in [("list", "cursor"), ("groups", "groups_cursor")] {
        if answer.to_string().len() <= shape::BUDGET {
            break;
        }
        let Some(mut table) = answer.get(key).cloned() else {
            continue;
        };
        let rows = table["rows"].as_array().cloned().unwrap_or_default();
        if rows.len() <= 1 {
            continue;
        }
        let start = shape::offset_named(query, cursor)?;
        let total = table["page"]["total"]
            .as_u64()
            .map_or(start + rows.len(), |n| n as usize);
        let (mut low, mut high) = (1, rows.len() - 1);
        let page = |shown: usize, table: &mut Value| {
            table["rows"] = json!(&rows[..shown]);
            table["page"] = json!({"returned":shown,"total":total,
                "next_cursor":(start+shown).to_string(),"cursor_parameter":cursor});
            table["note"] = json!(format!(
                "Follow next_cursor using {cursor} with the same filters."
            ));
        };
        while low < high {
            let middle = (low + high).div_ceil(2);
            page(middle, &mut table);
            answer[key] = table.clone();
            if answer.to_string().len() <= shape::BUDGET {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        page(low, &mut table);
        answer[key] = table;
    }
    shape::check_budget(answer)
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
