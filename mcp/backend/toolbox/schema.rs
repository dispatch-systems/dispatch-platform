//! The JSON Schemas the server publishes for a tool's arguments and its answer.
use super::{DSP, DSP_ABOUT, DSPS, DSPS_ABOUT, Scope};
use schemars::{JsonSchema, generate::SchemaSettings};
use serde_json::{Map, Value, json};
use std::any::type_name;

/// The JSON Schema of a tool's arguments or answer, in the draft MCP names, without the
/// type's own title and description, which say nothing to a model.
pub(super) fn of<T: JsonSchema>() -> Map<String, Value> {
    let schema = SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    let Value::Object(mut schema) = serde_json::to_value(schema).expect("a schema") else {
        panic!("{} has no schema object", type_name::<T>());
    };
    schema.remove("title");
    schema.remove("description");
    schema
}

/// The arguments a call about DSPs names them with, and their schema, for `scope`.
fn named(scope: Scope) -> Option<(&'static str, Value)> {
    match scope {
        Scope::Dsp => Some((DSP, json!({"type":"string","description":DSP_ABOUT}))),
        Scope::Dsps => Some((
            DSPS,
            json!({"type":"array","items":{"type":"string"},"description":DSPS_ABOUT}),
        )),
        Scope::Connection => None,
    }
}

/// The ways a tool's arguments may be written: the object itself, or each action of a choice
/// of them.
fn branches(schema: &mut Map<String, Value>) -> Vec<&mut Map<String, Value>> {
    match ["oneOf", "anyOf"]
        .into_iter()
        .find(|key| schema.get(*key).is_some_and(Value::is_array))
    {
        Some(key) => schema[key]
            .as_array_mut()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object_mut)
            .collect(),
        None => vec![schema],
    }
}

/// The JSON Schema of what a tool takes: an object, as MCP requires, and the `dsp` or `dsps`
/// a call about DSPs names, in each of its actions. A tool's own `dsp` or `dsps` stays as it
/// is, for the registry to refuse.
pub(super) fn input<T: JsonSchema>(scope: Scope) -> Map<String, Value> {
    let mut schema = of::<T>();
    schema
        .entry("type")
        .or_insert_with(|| Value::String("object".into()));
    // Every tool lists what it takes, nothing included, as some hosts insist.
    schema.entry("properties").or_insert_with(|| json!({}));
    if let Some((name, about)) = named(scope) {
        let choices = schema.contains_key("oneOf") || schema.contains_key("anyOf");
        if choices && let Some(properties) = schema["properties"].as_object_mut() {
            properties.entry(name).or_insert_with(|| about.clone());
        }
        for branch in branches(&mut schema) {
            if let Some(properties) = branch
                .entry("properties")
                .or_insert_with(|| json!({}))
                .as_object_mut()
            {
                properties.entry(name).or_insert_with(|| about.clone());
            }
        }
    }
    schema
}

/// Panics unless a tool takes an object that refuses fields it doesn't name, in every action,
/// leaves `dsp` and `dsps` to the server, and answers an object.
pub(super) fn check(
    name: &str,
    scope: Scope,
    input: &Map<String, Value>,
    output: &Map<String, Value>,
) {
    let mut input = input.clone();
    assert!(
        input["type"] == "object",
        "{name} takes something other than an object"
    );
    let ours = named(scope);
    let top = input.get("properties").cloned().unwrap_or_default();
    let shapes: Vec<Value> = branches(&mut input)
        .into_iter()
        .map(|branch| Value::Object(branch.clone()))
        .collect();
    for shape in shapes.iter().chain([&json!({"properties": top})]) {
        if shape.get("type").is_some() || shape.get("additionalProperties").is_some() {
            assert!(
                shape["additionalProperties"] == false,
                "{name} takes an object that refuses fields it doesn't name"
            );
        }
        for argument in [DSP, DSPS] {
            let given = &shape["properties"][argument];
            let expected = ours.as_ref().filter(|(ours, _)| *ours == argument);
            assert!(
                match expected {
                    Some((_, about)) => given == about,
                    None => given.is_null(),
                },
                "{name} names its own `{argument}`, which the server reads for a tool about DSPs"
            );
        }
    }
    assert_eq!(
        output.get("type"),
        Some(&json!("object")),
        "{name} answers something other than an object"
    );
}
