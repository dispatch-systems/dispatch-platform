use crate::{
    api::types::ToolLevel,
    testing::{self, agent, call_tool, serving},
    toolbox::Failure,
};
use dispatch_core::{
    accounts::Auth,
    foundation::config::Site,
    support::tickets,
    testing::{bootstrapped, platform_owner},
};
use serde_json::json;

#[tokio::test]
async fn ticket_diagnostics_are_read_only_private_and_bound_to_the_servers_dsp() {
    testing::install();
    let (_root, db, dsp) = bootstrapped();
    let owner = platform_owner(&db);
    let other = db
        .new_dsp("Other delivery", "America/Chicago", &owner, false)
        .unwrap();
    let auth = Auth {
        user: db.active_platform_owner(&owner).unwrap().unwrap(),
        hash: String::new(),
        csrf: String::new(),
        raw: String::new(),
        preview: None,
        site: Site::Admin,
        scope: Some(dsp.clone()),
    };
    let report = tickets::create(
        &db,
        &auth,
        &json!({
            "requestId":"diagnostics-test-id",
            "kind":"bug",
            "title":"Collection stuck",
            "description":"Private reporter sentinel: this must never appear in diagnostic output"
        }),
    )
    .unwrap();
    let caller = agent(
        &db,
        &[&dsp, &other.id],
        &[("support_ticket_diagnostics", ToolLevel::Read)],
    );
    let state = serving(db);
    let answer = call_tool(
        &state,
        &caller,
        "support_ticket_diagnostics",
        json!({"dsp":dsp,"ticket_id":report.ticket.id}),
    )
    .await
    .unwrap()
    .data;
    assert_eq!(answer["ticket_id"], report.ticket.id);
    assert!(!answer.to_string().contains("Private reporter sentinel"));
    // Nor who reported it: the agent reads the ticket's health, never its people.
    assert!(answer["diagnostics"].get("reporterName").is_none());
    assert_eq!(answer["diagnostics"]["dspId"], dsp);
    let refused = call_tool(
        &state,
        &caller,
        "support_ticket_diagnostics",
        json!({"dsp":other.id,"ticket_id":report.ticket.id}),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(refused, Failure::Refused(ref refusal) if refusal.code == "support_ticket_not_found")
    );
    // Caller-supplied time windows and record lists are outside this tool's contract.
    assert!(
        call_tool(
            &state,
            &caller,
            "support_ticket_diagnostics",
            json!({"dsp":dsp,"ticket_id":report.ticket.id,"from":"1900-01-01"})
        )
        .await
        .is_err()
    );
}
