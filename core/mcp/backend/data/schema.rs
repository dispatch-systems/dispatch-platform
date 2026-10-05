//! Response contracts owned by the same catalog as REST and MCP. Tables retain their
//! compact column/row format; schemas describe its structure without fixing dynamic metrics.
use serde_json::{Map, Value, json};

pub fn text() -> Value {
    json!({"type":"string"})
}
pub fn boolean() -> Value {
    json!({"type":"boolean"})
}
pub fn count() -> Value {
    json!({"type":"integer","minimum":0})
}
pub fn number() -> Value {
    json!({"type":"number"})
}
pub fn nullable(value: Value) -> Value {
    json!({"anyOf":[value, {"type":"null"}]})
}
pub fn array(items: Value) -> Value {
    json!({"type":"array","items":items})
}
pub fn map(value: Value) -> Value {
    json!({"type":"object","additionalProperties":value})
}
pub fn object(fields: &[(&str, Value)], required: &[&str]) -> Value {
    let properties: Map<String, Value> = fields
        .iter()
        .map(|(name, schema)| ((*name).into(), schema.clone()))
        .collect();
    json!({"type":"object","properties":properties,"required":required})
}

/// A DSP answer, including the interpretation and optional access/coverage explanations.
pub fn answer(fields: &[(&str, Value)], required: &[&str]) -> Value {
    let mut fields = fields.to_vec();
    fields.extend([
        ("understood", understood()),
        ("note", text()),
        ("bypassed", array(text())),
        ("not_allowed", array(text())),
        ("switched_off", array(text())),
    ]);
    let mut required = required.to_vec();
    required.push("understood");
    object(&fields, &required)
}
fn understood() -> Value {
    object(
        &[
            ("dsp", text()),
            ("today", text()),
            ("period", text()),
            ("from", text()),
            ("to", text()),
            ("days", json!({"type":["integer","string"]})),
            (
                "driver",
                json!({"anyOf":[text(), object(&[("code",text()),("name",text())], &["code","name"])]}),
            ),
            ("week", text()),
        ],
        &["dsp", "today"],
    )
}

/// Coverage describes availability, not whether a route snapshot is final.
pub fn coverage() -> Value {
    object(
        &[
            (
                "status",
                json!({"type":"string","enum":["complete","partial","missing","unavailable"]}),
            ),
            ("enabled", boolean()),
            ("collected", count()),
            ("of", count()),
            ("missing", array(text())),
            ("snapshots", array(text())),
            ("not_posted_yet", array(text())),
        ],
        &["status"],
    )
}
pub fn totals() -> Value {
    map(nullable(number()))
}

/// Column names appear once; each row follows that order. A page is present only when
/// more rows exist. Grouped results and details have independent next_cursor values.
pub fn table(columns: &[&str]) -> Value {
    let mut column = text();
    if !columns.is_empty() {
        column["enum"] = json!(columns);
    }
    object(
        &[
            ("columns", array(column)),
            ("rows", array(json!({"type":"array","items":{}}))),
            (
                "page",
                object(
                    &[
                        ("returned", count()),
                        ("total", count()),
                        ("next_cursor", text()),
                    ],
                    &["returned", "total"],
                ),
            ),
            ("note", text()),
        ],
        &["columns", "rows"],
    )
}
pub fn error() -> Value {
    object(
        &[
            ("error", text()),
            ("message", text()),
            ("choices", array(text())),
        ],
        &["error"],
    )
}
pub fn reads() -> Value {
    object(
        &[("areas", array(text())), ("bypass", boolean())],
        &["areas", "bypass"],
    )
}
pub fn whoami() -> Value {
    object(
        &[
            (
                "key",
                object(
                    &[
                        ("name", text()),
                        (
                            "access",
                            json!({"type":"string","enum":["read","operator"]}),
                        ),
                        ("expiresAt", nullable(text())),
                    ],
                    &["name", "access", "expiresAt"],
                ),
            ),
            ("environment", text()),
            ("now", text()),
            (
                "dsps",
                array(object(
                    &[
                        ("id", text()),
                        ("name", text()),
                        ("timezone", text()),
                        ("today", text()),
                        ("features", array(text())),
                        ("reads", reads()),
                    ],
                    &["id", "name", "timezone", "today", "features", "reads"],
                )),
            ),
        ],
        &["key", "environment", "now", "dsps"],
    )
}
pub fn status() -> Value {
    answer(
        &[(
            "sources",
            map(object(
                &[
                    ("enabled", boolean()),
                    ("reads", boolean()),
                    ("latestDay", nullable(text())),
                    ("latestWeek", nullable(text())),
                    ("collectedAt", nullable(text())),
                    ("checkedAt", nullable(text())),
                    ("periodFrom", nullable(text())),
                    ("periodTo", nullable(text())),
                ],
                &["enabled", "reads"],
            )),
        )],
        &["sources"],
    )
}
pub fn metrics() -> Value {
    object(
        &[
            (
                "metrics",
                array(object(
                    &[
                        ("name", text()),
                        ("source", text()),
                        ("unit", text()),
                        ("total", text()),
                        ("description", text()),
                    ],
                    &["name", "source", "unit", "total", "description"],
                )),
            ),
            (
                "glossary",
                array(object(
                    &[("term", text()), ("meaning", text())],
                    &["term", "meaning"],
                )),
            ),
        ],
        &["metrics", "glossary"],
    )
}
pub fn drivers() -> Value {
    answer(
        &[
            ("found", count()),
            (
                "drivers",
                table(&["code", "name", "match", "paycom", "amazon"]),
            ),
        ],
        &["found", "drivers"],
    )
}
pub fn driver() -> Value {
    answer(
        &[
            ("totals", totals()),
            ("coverage", map(coverage())),
            ("days", table(&[])),
        ],
        &["totals", "coverage", "days"],
    )
}
pub fn team() -> Value {
    answer(
        &[
            ("sorted", text()),
            ("totals", totals()),
            ("coverage", map(coverage())),
            ("rows", table(&[])),
        ],
        &["sorted", "totals", "coverage", "rows"],
    )
}
