//! What the MCP checks before an agent's tool runs, for tools that need features of the
//! product: one runs only at a DSP that has every feature it needs on, hidden from its members
//! or not; one about several DSPs reads only those with them on; and one that changes something
//! does so only for a key allowed to change with it, recorded in the activity log as made by
//! the platform owner, through the agent.
use dispatch_core::testing::{audit_actions, audits, bootstrapped, platform_owner};
use dispatch_mcp::{
    Caller, KeyStore,
    api::types::ToolLevel,
    testing::{agent, serving},
    toolbox::{Answer, AnyTool, Cx, Effect, Failure, Reply, Scope, Tool, Toolbox},
    tools::Nothing,
};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::sync::Arc;

#[derive(Serialize, schemars::JsonSchema)]
struct Seen {
    dsps: Vec<String>,
}
fn seen(cx: &Cx) -> Seen {
    Seen {
        dsps: cx.dsps().iter().map(|dsp| dsp.name.clone()).collect(),
    }
}
/// A tool that reads DVIC, and answers with the DSP it was handed.
struct Inspect;
impl Tool for Inspect {
    const NAME: &'static str = "inspect";
    const TITLE: &'static str = "Inspect";
    const DESCRIPTION: &'static str = "Answer with the DSP the call is about.";
    const FEATURES: &'static [&'static str] = &["dvic"];
    type Input = Nothing;
    type Output = Seen;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<Seen>> {
        Ok(seen(&cx).into())
    }
}
/// A tool that needs DVIC and Routes both.
struct Compare;
impl Tool for Compare {
    const NAME: &'static str = "compare";
    const TITLE: &'static str = "Compare";
    const DESCRIPTION: &'static str = "Answer with the DSP, where both features are on.";
    const FEATURES: &'static [&'static str] = &["dvic", "routes"];
    type Input = Nothing;
    type Output = Seen;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<Seen>> {
        Ok(seen(&cx).into())
    }
}
/// A tool about every DSP with DVIC on, which says where Routes is on too.
struct Survey;
impl Tool for Survey {
    const NAME: &'static str = "survey";
    const TITLE: &'static str = "Survey";
    const DESCRIPTION: &'static str = "Answer with the DSPs, and those with Routes.";
    const SCOPE: Scope = Scope::Dsps;
    const FEATURES: &'static [&'static str] = &["dvic"];
    type Input = Nothing;
    type Output = Value;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<Value>> {
        let mut routes = vec![];
        for dsp in cx.dsps() {
            if cx.has(dsp, "routes").await? {
                routes.push(dsp.name.clone());
            }
        }
        Ok(json!({"dsps": seen(&cx).dsps, "routes": routes}).into())
    }
}
/// A tool that changes something at a DSP with DVIC on, and says so in its activity log.
struct Mark;
impl Tool for Mark {
    const NAME: &'static str = "mark";
    const TITLE: &'static str = "Mark";
    const DESCRIPTION: &'static str = "Mark the DSP.";
    const FEATURES: &'static [&'static str] = &["dvic"];
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = Seen;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<Seen>> {
        let dsp = cx.dsp().id.clone();
        cx.write(move |w| Ok(w.audit(&dsp, "dvic.marked", "Marked", None, &[])?))
            .await?;
        Ok(seen(&cx).into())
    }
}

fn tools() -> Toolbox {
    Toolbox::of(vec![&Inspect as &dyn AnyTool, &Compare, &Survey, &Mark])
}
async fn call(
    state: &Arc<dispatch_core::State>,
    caller: &Caller,
    name: &str,
    dsp: Option<&str>,
) -> Answer<Value> {
    let mut arguments = Map::new();
    if let Some(dsp) = dsp {
        arguments.insert("dsp".into(), json!(dsp));
    }
    tools()
        .call(state.clone(), caller.clone(), name.to_owned(), arguments)
        .await
        .answer
        .map(|answered| answered.data)
}
fn refusal(answer: Answer<Value>) -> (String, String) {
    match answer {
        Err(Failure::Refused(refusal)) => (refusal.code, refusal.message),
        other => panic!("not refused: {other:?}"),
    }
}
async fn offered(state: &Arc<dispatch_core::State>, caller: &Caller) -> Vec<&'static str> {
    let caller = caller.clone();
    state
        .read(move |db| {
            Ok(tools()
                .offered(db, &caller)?
                .iter()
                .map(|offered| offered.tool.name())
                .collect())
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_tool_runs_only_where_every_feature_it_needs_is_on_hidden_or_not() {
    dispatch_backend::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    let other = db.new_dsp("Other DSP", "UTC", &owner, false).unwrap().id;
    db.set_feature(&north, "dvic", true, &owner).unwrap();
    db.set_feature(&north, "routes", false, &owner).unwrap();
    db.set_feature(&other, "dvic", false, &owner).unwrap();
    db.set_feature(&other, "routes", true, &owner).unwrap();
    let reader = agent(&db, &[&north, &other], &[]);
    let name = |dsp: &str| db.find_dsp(dsp).unwrap().name;
    let (north_name, other_name) = (name(&north), name(&other));
    let state = serving(db);
    let set = |dsp: &str, feature: &'static str, on: bool| {
        let (state, dsp, owner) = (state.clone(), dsp.to_owned(), owner.clone());
        async move {
            state
                .run(move |db| db.set_feature(&dsp, feature, on, &owner).map(|_| ()))
                .await
                .unwrap()
        }
    };

    assert_eq!(
        offered(&state, &reader).await,
        ["inspect", "survey", "mark"]
    );
    assert_eq!(
        call(&state, &reader, "inspect", Some(&north))
            .await
            .unwrap(),
        json!({"dsps": [north_name]})
    );
    let (code, message) = refusal(call(&state, &reader, "inspect", Some(&other)).await);
    assert_eq!(code, "switched_off");
    assert!(
        message.starts_with("DVIC is switched off at Other DSP."),
        "{message}"
    );
    // The Activity log notes the DSP a refused call was about.
    let mut arguments = Map::new();
    arguments.insert("dsp".into(), json!(other));
    let refused = tools()
        .call(state.clone(), reader.clone(), "inspect".into(), arguments)
        .await;
    assert_eq!(refused.dsp.map(|dsp| dsp.name), Some(other_name.clone()));
    // A tool that needs two features runs only where both are on.
    let (code, message) = refusal(call(&state, &reader, "compare", Some(&north)).await);
    assert_eq!(code, "switched_off");
    assert!(
        message.starts_with("Routes is switched off at"),
        "{message}"
    );
    set(&north, "routes", true).await;
    assert!(call(&state, &reader, "compare", Some(&north)).await.is_ok());
    assert_eq!(
        offered(&state, &reader).await,
        ["inspect", "compare", "survey", "mark"]
    );
    // About several DSPs, it reads those with its feature on, and asks about another.
    assert_eq!(
        call(&state, &reader, "survey", None).await.unwrap(),
        json!({"dsps": [north_name], "routes": [north_name]})
    );
    set(&other, "dvic", true).await;
    let survey = call(&state, &reader, "survey", None).await.unwrap();
    assert_eq!(survey["routes"].as_array().unwrap().len(), 2, "{survey}");
    assert!(
        survey["dsps"]
            .as_array()
            .unwrap()
            .contains(&json!(other_name))
    );
    // Hidden from the DSP's members, it still runs for agents.
    let (state_, north_, owner_) = (state.clone(), north.clone(), owner.clone());
    state_
        .run(move |db| db.show_feature(&north_, "dvic", false, &owner_).map(|_| ()))
        .await
        .unwrap();
    assert!(call(&state, &reader, "inspect", Some(&north)).await.is_ok());
    // Switched off everywhere it reaches, nothing that needs it is offered.
    set(&north, "dvic", false).await;
    set(&other, "dvic", false).await;
    assert!(offered(&state, &reader).await.is_empty());
    assert_eq!(
        refusal(call(&state, &reader, "inspect", Some(&north)).await).0,
        "switched_off"
    );
    assert_eq!(
        refusal(call(&state, &reader, "survey", None).await).0,
        "switched_off"
    );
}

#[tokio::test]
async fn a_tool_that_changes_something_does_so_only_for_a_key_allowed_to() {
    dispatch_backend::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    db.set_feature(&north, "dvic", true, &owner).unwrap();
    // Taking new tools, a key reads with one added since, and changes nothing with it.
    let reader = agent(&db, &[&north], &[]);
    let operator = agent(&db, &[&north], &[]);
    // These tools are a test's, so no request can name them: the key's choice is written as
    // the Agents page would.
    db.platform
        .exec(
            "INSERT INTO agent_key_tools(key_id,tool,allowed,changes) VALUES (?,'mark',1,1)",
            [&operator.key],
        )
        .unwrap();
    let state = serving(db);
    let (code, message) = refusal(call(&state, &reader, "mark", Some(&north)).await);
    assert_eq!(code, "not_allowed");
    assert!(message.contains("may only read with mark"), "{message}");
    let at = north.clone();
    let marks = state
        .read(move |db| Ok(audit_actions(db, &at, "dvic.marked")))
        .await
        .unwrap();
    assert!(marks.is_empty());

    // Allowed to change with it, it runs for that key alone.
    assert!(call(&state, &operator, "mark", Some(&north)).await.is_ok());
    // The platform owner's own, as any change they make at a DSP is.
    let events = state.read(|db| audits(db, None)).await.unwrap();
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
    // Its listing says it may change something only to the key allowed to.
    let level = |caller: &Caller| {
        let caller = caller.clone();
        let state = state.clone();
        async move {
            state
                .read(move |db| {
                    let caller = db.revalidate_agent(&caller)?;
                    Ok(tools()
                        .offered(db, &caller)?
                        .into_iter()
                        .find(|offered| offered.tool.name() == "mark")
                        .map(|offered| offered.level))
                })
                .await
                .unwrap()
        }
    };
    assert_eq!(level(&reader).await, Some(ToolLevel::Read));
    assert_eq!(level(&operator).await, Some(ToolLevel::Change));
}
