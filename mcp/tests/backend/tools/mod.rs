use super::*;
use crate::{
    KeyStore,
    api::types::{AgentAccess, AgentKeyRequest},
};
use dispatch_core::{foundation::config::Environment, tenancy::api::types::DspStatus};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Word {
    /// The word to answer with.
    word: String,
}
#[derive(Serialize, JsonSchema)]
struct Said {
    said: String,
}
/// A tool about a DSP, as a feature declares one.
struct Echo;
impl Tool for Echo {
    const NAME: &'static str = "echo";
    const TITLE: &'static str = "Echo";
    const DESCRIPTION: &'static str = "Answer with the word given.";
    type Input = Word;
    type Output = Said;
    fn call(_: &Cx, input: Word) -> Answer<Said> {
        Ok(Said { said: input.word })
    }
}

fn dsp(id: &str, name: &str) -> Dsp {
    Dsp {
        id: id.into(),
        name: name.into(),
        environment: Environment::Preview,
        status: DspStatus::Active,
        timezone: "UTC".into(),
        permanent: false,
        revision: 1,
        created_at: "2026-10-01T00:00:00.000Z".into(),
        code: None,
    }
}
fn caller(dsps: Vec<Dsp>) -> Caller {
    Caller {
        key: "agentkey_a".into(),
        name: "Laptop".into(),
        user: "user_a".into(),
        access: AgentAccess::Read,
        tools: Grants {
            all: true,
            chosen: HashMap::new(),
        },
        expires_at: None,
        dsps,
        client: "curl".into(),
    }
}

#[test]
fn a_dsp_tool_takes_dsp_beside_its_own_arguments_and_nothing_else() {
    let schema = Value::Object(Echo.input_schema());
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["word"],
        json!({"type":"string","description":"The word to answer with."})
    );
    assert_eq!(schema["properties"][DSP]["type"], "string");
    assert_eq!(schema["required"], json!(["word"]));
    // Core's own tools are about the connection, and take no DSP.
    for tool in connection::TOOLS {
        assert!(tool.input_schema()["properties"].get(DSP).is_none());
    }
}

#[test]
fn a_call_names_a_dsp_the_connection_reaches_or_reaches_only_one() {
    let two = caller(vec![
        dsp("dsp_n", "Northline Logistics"),
        dsp("dsp_s", "Summit"),
    ]);
    let picked = |named: Value| pick(&two, Some(named)).map(|dsp| dsp.id.clone());
    assert_eq!(picked(json!("dsp_s")).unwrap(), "dsp_s");
    assert_eq!(picked(json!(" northline logistics ")).unwrap(), "dsp_n");
    let refused = |result: Result<&Dsp, Refusal>| {
        let refusal = result.unwrap_err();
        (refusal.code, refusal.choices)
    };
    let both = vec!["Northline Logistics".to_owned(), "Summit".to_owned()];
    assert_eq!(
        refused(pick(&two, Some(json!("Harbor")))),
        ("dsp_not_found".into(), both.clone())
    );
    assert_eq!(
        refused(pick(&two, None)),
        ("dsp_required".into(), both.clone())
    );
    assert_eq!(
        refused(pick(&two, Some(json!(7)))),
        ("invalid_input".into(), both)
    );
    // Reaching one, a call may leave it out; reaching none, nothing can be asked.
    let one = caller(vec![dsp("dsp_n", "Northline Logistics")]);
    assert_eq!(pick(&one, Some(Value::Null)).unwrap().id, "dsp_n");
    assert_eq!(refused(pick(&caller(vec![]), None)).0, "no_dsp");
}

#[test]
fn a_tool_is_handed_its_arguments_and_refuses_ones_it_does_not_name() {
    crate::testing::install();
    let (_root, db) = dispatch_core::testing::store();
    let one = caller(vec![dsp("dsp_n", "Northline Logistics")]);
    let tools = Toolbox::of(vec![Listed {
        tool: &Echo,
        feature: None,
    }]);
    let call = |arguments: Value| {
        let Value::Object(arguments) = arguments else {
            unreachable!()
        };
        tools.invoke(&db, &one, "echo", arguments)
    };
    let answered = call(json!({"word":"hello","dsp":"dsp_n"}));
    assert_eq!(answered.answer.unwrap(), json!({"said":"hello"}));
    assert_eq!(answered.dsp.unwrap().name, "Northline Logistics");
    let Err(Failure::Refused(refusal)) = call(json!({"word":"hello","driver":"Avery"})).answer
    else {
        panic!("an argument it doesn't name was let through")
    };
    assert_eq!(refusal.code, "invalid_input");
    assert!(refusal.message.contains("driver"), "{}", refusal.message);
    let Err(Failure::Refused(refusal)) = tools.invoke(&db, &one, "nope", Map::new()).answer else {
        panic!("a tool no one declared was called")
    };
    assert_eq!(
        (refusal.code.as_str(), refusal.choices),
        ("unknown_tool", vec!["echo".into()])
    );
}

struct Misnamed;
impl Tool for Misnamed {
    const NAME: &'static str = "Find Drivers";
    const TITLE: &'static str = "Find drivers";
    const DESCRIPTION: &'static str = "Find drivers.";
    type Input = Word;
    type Output = Said;
    fn call(_: &Cx, input: Word) -> Answer<Said> {
        Ok(Said { said: input.word })
    }
}
#[derive(Deserialize, JsonSchema)]
struct Loose {
    word: String,
}
struct Lenient;
impl Tool for Lenient {
    const NAME: &'static str = "lenient";
    const TITLE: &'static str = "Lenient";
    const DESCRIPTION: &'static str = "Take any argument.";
    type Input = Loose;
    type Output = Said;
    fn call(_: &Cx, input: Loose) -> Answer<Said> {
        Ok(Said { said: input.word })
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OwnDsp {
    dsp: String,
}
struct Chooser;
impl Tool for Chooser {
    const NAME: &'static str = "chooser";
    const TITLE: &'static str = "Chooser";
    const DESCRIPTION: &'static str = "Choose its own DSP.";
    type Input = OwnDsp;
    type Output = Said;
    fn call(_: &Cx, input: OwnDsp) -> Answer<Said> {
        Ok(Said { said: input.dsp })
    }
}
struct Elsewhere;
impl Tool for Elsewhere {
    const NAME: &'static str = "elsewhere";
    const TITLE: &'static str = "Elsewhere";
    const DESCRIPTION: &'static str = "Belong to another feature's part.";
    const PART: Option<&'static str> = Some("timecard.meals");
    type Input = Word;
    type Output = Said;
    fn call(_: &Cx, input: Word) -> Answer<Said> {
        Ok(Said { said: input.word })
    }
}

#[test]
#[should_panic(expected = "is not lowercase words joined by _")]
fn a_tool_is_called_by_lowercase_words() {
    check(&[&Misnamed]);
}
#[test]
#[should_panic(expected = "two tools are called echo")]
fn two_tools_never_share_a_name() {
    check(&[&Echo, &Echo]);
}
#[test]
#[should_panic(expected = "lenient takes an object that refuses fields it doesn't name")]
fn a_tool_refuses_arguments_it_does_not_name() {
    check(&[&Lenient]);
}
#[test]
#[should_panic(expected = "chooser names its own `dsp`")]
fn a_dsp_tool_leaves_choosing_the_dsp_to_core() {
    check(&[&Chooser]);
}
#[test]
#[should_panic(expected = "elsewhere belongs to timecard.meals, which is no part of the feature")]
fn a_tool_belongs_to_a_part_of_its_own_feature() {
    check(&[&Elsewhere]);
}

/// Calls an installed tool as `caller` now is, with the DSP it reaches.
fn called(db: &Store, caller: &Caller, name: &str) -> Answer<Value> {
    let caller = db.revalidate_agent(caller).unwrap();
    crate::testing::call_tool(db, &caller, name, json!({}))
}
fn code(answer: Answer<Value>) -> String {
    match answer {
        Err(Failure::Refused(refusal)) => refusal.code,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_key_uses_the_tools_chosen_for_it_and_new_ones_only_when_they_read() {
    crate::testing::install();
    let (_root, db, dsp) = dispatch_core::testing::bootstrapped();
    let owner = dispatch_core::testing::platform_owner(&db);
    // A new key starts with every tool that only reads.
    assert_eq!(Toolbox::installed().defaults(), ["stand_in_read"]);
    let caller = crate::testing::agent(&db, &[&dsp], &["stand_in_read"]);
    let listed = db.agent_keys(&HashMap::new()).unwrap();
    let names: Vec<&str> = listed.tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["stand_in_read", "stand_in_change"]);
    assert!(listed.tools[1].changes);
    assert_eq!(listed.keys[0].tools, ["stand_in_read"]);
    assert!(called(&db, &caller, "stand_in_read").is_ok());
    assert_eq!(code(called(&db, &caller, "stand_in_change")), "not_allowed");
    // Core's own tools are always the connection's.
    assert!(called(&db, &caller, "whoami").is_ok());

    // Switched on afterwards, a tool that changes something runs on the next call.
    let request = |tools: &[&str], all: bool| {
        AgentKeyRequest::parse(
            &json!({"name": caller.name, "allDsps": false, "dsps": [dsp],
            "access": "read", "allTools": all, "tools": tools, "expiresAt": null}),
        )
        .unwrap()
    };
    let both = request(&["stand_in_change", "stand_in_read"], true);
    db.update_agent_key(&owner, &caller.key, &both).unwrap();
    assert!(called(&db, &caller, "stand_in_change").is_ok());
    assert_eq!(
        dispatch_core::testing::audit_actions(&db, &dsp, "stand_in."),
        ["stand_in.changed"]
    );
    let edits = dispatch_core::testing::audits(&db, None).unwrap();
    let edited = edits
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["action"] == "agent.key_updated")
        .unwrap();
    assert_eq!(
        edited["changes"],
        json!([{"field":"tools","from":"[stand_in_read]",
            "to":"[stand_in_change, stand_in_read]"}])
    );

    // A tool added since the key's tools were chosen has no choice of its own: it is allowed
    // when it only reads and the key takes new tools, and one that changes something never is.
    db.platform
        .exec("DELETE FROM agent_key_tools WHERE key_id=?", [&caller.key])
        .unwrap();
    assert!(called(&db, &caller, "stand_in_read").is_ok());
    assert_eq!(code(called(&db, &caller, "stand_in_change")), "not_allowed");
    db.update_agent_key(&owner, &caller.key, &request(&[], false))
        .unwrap();
    db.platform
        .exec("DELETE FROM agent_key_tools WHERE key_id=?", [&caller.key])
        .unwrap();
    assert_eq!(code(called(&db, &caller, "stand_in_read")), "not_allowed");

    // As the platform owners' emails word it.
    let toolbox = Toolbox::installed();
    let grants = |all: bool, chosen: &[(&str, bool)]| Grants {
        all,
        chosen: chosen
            .iter()
            .map(|(tool, on)| ((*tool).to_owned(), *on))
            .collect(),
    };
    assert_eq!(toolbox.summary(&grants(false, &[])), "None of 2");
    assert_eq!(
        toolbox.summary(&grants(true, &[])),
        "1 of 2; and new ones that only read"
    );
    assert_eq!(
        toolbox.summary(&grants(
            false,
            &[("stand_in_read", true), ("stand_in_change", true)]
        )),
        "All 2, 1 that makes changes"
    );

    // Only a tool there is can be chosen.
    let unknown = request(&["drive_truck"], true);
    let refused = db.update_agent_key(&owner, &caller.key, &unknown);
    assert_eq!(refused.unwrap_err().code, "invalid_input");
}
