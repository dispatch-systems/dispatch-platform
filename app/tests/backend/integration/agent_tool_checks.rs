//! What core checks before an agent's tool runs, for a tool a feature declares: it runs only at
//! a DSP that has the feature on, hidden from its members or not, and one that changes
//! something only for a key it was switched on for, recorded in the activity log as made by
//! the platform owner, through the agent.
use dispatch_core::{
    db::Store,
    testing::{self as common, audits, bootstrapped, platform_owner},
};
use dispatch_mcp::{
    Caller,
    testing::agent,
    tools::{Answer, Cx, Effect, Failure, Listed, Nothing, Tool, Toolbox},
};
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Serialize, schemars::JsonSchema)]
struct Seen {
    dsp: String,
}
/// A tool DVIC could declare: it reads, and answers with the DSP it was handed.
struct Inspect;
impl Tool for Inspect {
    const NAME: &'static str = "inspect";
    const TITLE: &'static str = "Inspect";
    const DESCRIPTION: &'static str = "Answer with the DSP the call is about.";
    type Input = Nothing;
    type Output = Seen;
    fn call(cx: &Cx, _: Nothing) -> Answer<Seen> {
        Ok(Seen {
            dsp: cx.dsp().name.clone(),
        })
    }
}
/// A tool DVIC could declare that changes something, and says so in the activity log.
struct Mark;
impl Tool for Mark {
    const NAME: &'static str = "mark";
    const TITLE: &'static str = "Mark";
    const DESCRIPTION: &'static str = "Mark the DSP.";
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = Seen;
    fn call(cx: &Cx, _: Nothing) -> Answer<Seen> {
        cx.audit("dvic.marked", "Marked", None, &[])?;
        Ok(Seen {
            dsp: cx.dsp().name.clone(),
        })
    }
}

fn tools() -> Toolbox {
    Toolbox::of(
        [&Inspect as &dyn dispatch_mcp::tools::AnyTool, &Mark]
            .into_iter()
            .map(|tool| Listed {
                tool,
                feature: Some(&dispatch_dvic::FEATURE),
            })
            .collect(),
    )
}
fn call(db: &Store, caller: &Caller, name: &str, dsp: &str) -> Answer<Value> {
    let mut arguments = Map::new();
    arguments.insert("dsp".into(), json!(dsp));
    tools().invoke(db, caller, name, arguments).answer
}
fn refusal(answer: Answer<Value>) -> (String, String) {
    match answer {
        Err(Failure::Refused(refusal)) => (refusal.code, refusal.message),
        other => panic!("not refused: {other:?}"),
    }
}
fn offered(db: &Store, caller: &Caller) -> Vec<&'static str> {
    tools()
        .offered(db, caller)
        .unwrap()
        .iter()
        .map(|listed| listed.tool.name())
        .collect()
}

#[test]
fn a_tool_runs_only_where_its_feature_is_on_hidden_or_not() {
    dispatch_backend::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    let other = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
    db.set_feature(&north, "dvic", true, &owner).unwrap();
    let reader = agent(&db, &[&north, &other], &[]);
    let name = |dsp: &str| db.find_dsp(dsp).unwrap().name;

    assert_eq!(offered(&db, &reader), ["inspect"]);
    assert_eq!(
        call(&db, &reader, "inspect", &north).unwrap(),
        json!({"dsp": name(&north)})
    );
    let (code, message) = refusal(call(&db, &reader, "inspect", &other));
    assert_eq!(code, "switched_off");
    assert!(
        message.starts_with("DVIC is switched off at Other DSP."),
        "{message}"
    );
    // Hidden from the DSP's members, it still runs for agents.
    db.show_feature(&north, "dvic", false, &owner).unwrap();
    assert!(call(&db, &reader, "inspect", &north).is_ok());
    // Switched off everywhere it reaches, nothing of it is offered.
    db.set_feature(&north, "dvic", false, &owner).unwrap();
    assert!(offered(&db, &reader).is_empty());
    assert_eq!(
        refusal(call(&db, &reader, "inspect", &north)).0,
        "switched_off"
    );
}

#[test]
fn a_tool_that_changes_something_runs_only_where_switched_on_as_its_agent() {
    dispatch_backend::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    db.set_feature(&north, "dvic", true, &owner).unwrap();
    let reader = agent(&db, &[&north], &[]);
    let (code, message) = refusal(call(&db, &reader, "mark", &north));
    assert_eq!(code, "not_allowed");
    assert!(message.contains("may not use mark"), "{message}");
    assert_eq!(offered(&db, &reader), ["inspect"]);
    assert!(common::audit_actions(&db, &north, "dvic.marked").is_empty());

    // Switched on for one key, it runs for that key alone.
    let mut operator = agent(&db, &[&north], &[]);
    operator.tools.chosen.insert("mark".into(), true);
    assert_eq!(offered(&db, &operator), ["inspect", "mark"]);
    assert!(call(&db, &operator, "mark", &north).is_ok());
    // The platform owner's own, as any change they make at a DSP is.
    let events = audits(&db, None).unwrap();
    let marked = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["action"] == "dvic.marked")
        .unwrap_or_else(|| panic!("{events}"));
    assert_eq!(marked["actorName"], "Test Owner", "{marked}");
    assert_eq!(
        marked["changes"],
        json!([{"field":"via","from":null,"to":operator.name}]),
        "{marked}"
    );
}
