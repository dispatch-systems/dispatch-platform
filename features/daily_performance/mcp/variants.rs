//! Daily alternatives to focused tools. Source selection happens before permission checks.
use super::{DAILY_FEEDBACK, DAILY_RETURNS, DAILY_SAFETY, catalog, views};
use dispatch_core::{
    State,
    db::Store,
    mcp::{
        Caller,
        data::{
            Answer,
            catalog::{self as common, Variant},
        },
    },
};
use serde_json::{Value, json};
pub const ENDPOINTS: &[Variant] = &[
    Variant {
        endpoint: "feedback",
        area: DAILY_FEEDBACK,
        params: catalog::FEEDBACK_PARAMS,
        output: catalog::output,
        answer: |db, state, caller, _, query| {
            focused(db, state, caller, query, "feedback", "driver_feedback")
        },
    },
    Variant {
        endpoint: "safety",
        area: DAILY_SAFETY,
        params: catalog::SAFETY_PARAMS,
        output: catalog::output,
        answer: |db, state, caller, _, query| {
            focused(db, state, caller, query, "safety", "safety_events")
        },
    },
    Variant {
        endpoint: "returns",
        area: DAILY_RETURNS,
        params: catalog::RETURNS_PARAMS,
        output: catalog::output,
        answer: |db, state, caller, _, query| {
            focused(db, state, caller, query, "returns", "returns_to_station")
        },
    },
];
fn focused(
    db: &Store,
    state: &State,
    caller: &Caller,
    query: &Value,
    id: &str,
    dataset: &str,
) -> Answer {
    common::check(id, query)?;
    if query.get("dataset").is_some_and(|value| value != dataset) {
        return Err(dispatch_core::mcp::data::Refusal::new(
            400,
            "invalid_dataset",
            "This focused tool reads its named dataset.",
        )
        .into());
    }
    let mut query = query.clone();
    query["dataset"] = json!(dataset);
    if id == "feedback" && query.get("feedback").is_none() {
        query["feedback"] = json!("negative");
    }
    views::read(db, state, caller, &query)
}
