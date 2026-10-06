//! Operational semantics are projections of daily data, never weekly scoring rules.
use super::{DAILY_FEEDBACK, DAILY_RETURNS, DAILY_SAFETY, views};
use dispatch_core::{
    State,
    db::Store,
    mcp::{
        Caller,
        data::{Answer, performance::Adapter},
    },
};
use serde_json::{Value, json};
pub const ADAPTERS: &[Adapter] = &[
    Adapter {
        endpoint: "returns",
        view: "operational",
        cursor_prefix: "operational",
        default_period: "yesterday",
        compare_default: false,
        normalize: |_| {},
        area: DAILY_RETURNS,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "returns_to_station"),
    },
    Adapter {
        endpoint: "safety",
        view: "operational",
        cursor_prefix: "operational",
        default_period: "yesterday",
        compare_default: false,
        normalize: |_| {},
        area: DAILY_SAFETY,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "safety_events"),
    },
    Adapter {
        endpoint: "feedback",
        view: "operational",
        cursor_prefix: "operational",
        default_period: "yesterday",
        compare_default: false,
        normalize: |_| {},
        area: DAILY_FEEDBACK,
        answer: |db, state, caller, _, query| read(db, state, caller, query, "driver_feedback"),
    },
];
fn read(db: &Store, state: &State, caller: &Caller, query: &Value, dataset: &str) -> Answer {
    let mut query = query.clone();
    query["dataset"] = json!(dataset);
    if dataset == "driver_feedback" && query.get("feedback").is_none() {
        query["feedback"] = json!("negative");
    }
    views::operational(db, state, caller, &query)
}
