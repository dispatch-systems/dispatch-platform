//! Whoami, called as an agent calls it.
use crate::{
    api::types::ToolLevel,
    testing::{self, agent, call_tool, serving},
};
use dispatch_core::testing::bootstrapped;
use serde_json::json;

#[tokio::test]
async fn answers_the_connection_its_dsps_and_what_it_may_do_with_each_tool() {
    testing::install();
    let (_root, db, dsp) = bootstrapped();
    let caller = agent(
        &db,
        &[&dsp],
        &[
            ("stand_in_read", ToolLevel::Read),
            ("stand_in_actions", ToolLevel::Change),
        ],
    );
    let name = db.find_dsp(&dsp).unwrap().name;
    let state = serving(db);
    let me = call_tool(&state, &caller, "whoami", json!({}))
        .await
        .unwrap()
        .data;
    assert_eq!(me["key"]["name"], caller.name.as_str());
    assert_eq!(me["key"]["access"], "read");
    assert_eq!(me["dsps"][0]["name"], name.as_str());
    let tools = &me["key"]["tools"];
    assert_eq!(tools["whoami"], "read");
    assert_eq!(tools["stand_in_read"], "read");
    assert_eq!(tools["stand_in_actions"], "change");
    // A tool it may not use isn't named.
    assert!(tools.get("stand_in_change").is_none(), "{tools}");
}
