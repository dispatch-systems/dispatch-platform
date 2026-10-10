use super::*;
use crate::{
    KeyStore,
    api::types::{AgentAccess, AgentKeyRequest},
    testing::{agent, call_tool, serving},
    tools::Nothing,
};
use dispatch_core::{
    foundation::config::Environment,
    tenancy::api::types::DspStatus,
    testing::{audit_actions, audits, bootstrapped, platform_owner},
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

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
/// A tool about a DSP.
struct Echo;
impl Tool for Echo {
    const NAME: &'static str = "echo";
    const TITLE: &'static str = "Echo";
    const DESCRIPTION: &'static str = "Answer with the word given.";
    type Input = Word;
    type Output = Said;
    async fn call(_: Cx, input: Word) -> Answer<Reply<Said>> {
        Ok(Said { said: input.word }.into())
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
fn schema(tool: &dyn AnyTool) -> Value {
    Value::Object(tool.input_schema())
}

#[test]
fn a_dsp_tool_takes_dsp_beside_its_own_arguments_and_nothing_else() {
    let schema = schema(&Echo);
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["word"],
        json!({"type":"string","description":"The word to answer with."})
    );
    assert_eq!(schema["properties"][DSP]["type"], "string");
    assert_eq!(schema["required"], json!(["word"]));
    // The tools about the connection take no DSP.
    for tool in crate::tools::TOOLS {
        assert!(tool.input_schema()["properties"].get(DSP).is_none());
    }
    // A tool about several takes a list of them.
    let across = Value::Object(crate::testing::StandInAcross.input_schema());
    assert_eq!(
        across["properties"][DSPS]["items"],
        json!({"type":"string"})
    );
    assert!(across["properties"].get(DSP).is_none());
}

#[test]
fn a_tool_of_several_actions_lists_each_with_its_arguments_and_the_dsp() {
    let schema = schema(&crate::testing::StandInActions);
    // Every host takes an object, whatever its choices.
    assert_eq!(schema["type"], "object");
    let actions = schema["oneOf"].as_array().unwrap();
    assert_eq!(actions.len(), 3);
    for action in actions {
        assert_eq!(action["additionalProperties"], false, "{action}");
        assert_eq!(action["properties"][DSP]["type"], "string", "{action}");
    }
    let mark = actions
        .iter()
        .find(|action| action["properties"]["action"]["const"] == "mark")
        .unwrap();
    assert_eq!(
        mark["properties"]["note"],
        json!({"type":"string","description":"What the mark says."})
    );
    assert_eq!(schema["properties"][DSP]["type"], "string");
    // The tool says what each does.
    let effect = |arguments: Value| {
        let Value::Object(arguments) = arguments else {
            unreachable!()
        };
        crate::testing::StandInActions
            .read(arguments)
            .unwrap()
            .effect
    };
    assert_eq!(effect(json!({"action":"look"})), Effect::Reads);
    assert_eq!(
        effect(json!({"action":"mark","note":"Checked"})),
        Effect::Changes
    );
    let Value::Object(wrong) = json!({"action":"mark"}) else {
        unreachable!()
    };
    assert_eq!(
        crate::testing::StandInActions
            .read(wrong)
            .err()
            .unwrap()
            .code,
        "invalid_input"
    );
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

fn refusal<T: std::fmt::Debug>(answer: Answer<T>) -> (String, String) {
    match answer {
        Err(Failure::Refused(refusal)) => (refusal.code, refusal.message),
        other => panic!("not refused: {other:?}"),
    }
}
fn data(answer: Answer<Answered>) -> Value {
    answer.unwrap().data
}

#[tokio::test]
async fn a_tool_is_handed_its_arguments_and_refuses_ones_it_does_not_name() {
    crate::testing::install();
    let (_root, db, north) = bootstrapped();
    let reader = agent(&db, &[&north], &[]);
    let state = serving(db);
    let tools = Toolbox::of(vec![&Echo]);
    let call = |arguments: Value| {
        let Value::Object(arguments) = arguments else {
            unreachable!()
        };
        tools.call(state.clone(), reader.clone(), "echo".into(), arguments)
    };
    let answered = call(json!({"word":"hello","dsp":north})).await;
    assert_eq!(data(answered.answer), json!({"said":"hello"}));
    assert_eq!(answered.dsp.unwrap().id, north);
    let (code, message) = refusal(call(json!({"word":"hello","driver":"Avery"})).await.answer);
    assert_eq!(code, "invalid_input");
    assert!(message.contains("driver"), "{message}");
    let unknown = tools
        .call(state.clone(), reader.clone(), "nope".into(), Map::new())
        .await;
    let Err(Failure::Refused(unknown)) = unknown.answer else {
        panic!("a tool no one declared was called")
    };
    assert_eq!(
        (unknown.code.as_str(), unknown.choices),
        ("unknown_tool", vec!["echo".into()])
    );
    // A key revoked hears so first, whatever else is wrong with its call.
    let (key, owner_of) = (reader.key.clone(), reader.user.clone());
    state
        .run(move |db| db.revoke_agent_key(&owner_of, &key).map(|_| ()))
        .await
        .unwrap();
    for (name, arguments) in [("nope", json!({})), ("echo", json!({"driver":"Avery"}))] {
        let Value::Object(arguments) = arguments else {
            unreachable!()
        };
        let called = tools
            .call(state.clone(), reader.clone(), name.into(), arguments)
            .await;
        assert_eq!(refusal(called.answer).0, "agent_key_revoked", "{name}");
    }
}

#[tokio::test]
async fn a_tool_about_several_dsps_reads_those_named_or_every_one_it_reaches() {
    crate::testing::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    let summit = db.new_dsp("Summit", "UTC", &owner, false).unwrap().id;
    let across = [("stand_in_across", ToolLevel::Read)];
    let both = agent(&db, &[&north, &summit], &across);
    let alone = agent(&db, &[&north], &across);
    let north_name = db.find_dsp(&north).unwrap().name;
    let state = serving(db);
    let across = |caller: &Caller, arguments: Value| {
        let (state, caller) = (state.clone(), caller.clone());
        async move { call_tool(&state, &caller, "stand_in_across", arguments).await }
    };
    let mut every = vec![north_name.clone(), "Summit".to_owned()];
    every.sort_by_key(|name| name.to_lowercase());
    assert_eq!(data(across(&both, json!({})).await), json!({"dsps": every}));
    assert_eq!(
        data(across(&both, json!({"dsps": ["summit", summit]})).await),
        json!({"dsps": ["Summit"]})
    );
    let (code, _) = refusal(across(&alone, json!({"dsps": ["Summit"]})).await);
    assert_eq!(code, "dsp_not_found");
    let (code, _) = refusal(across(&both, json!({"dsps": "Summit"})).await);
    assert_eq!(code, "invalid_input");
    // Its call is about more than one DSP, so the Activity log names none.
    let Value::Object(arguments) = json!({}) else {
        unreachable!()
    };
    let called = Toolbox::installed()
        .call(
            state.clone(),
            both.clone(),
            "stand_in_across".into(),
            arguments,
        )
        .await;
    assert!(called.dsp.is_none());
}

#[tokio::test]
async fn a_key_reads_or_changes_with_each_tool_as_the_owner_chose() {
    crate::testing::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    // A new key starts with every tool, to read: the stand-ins among them.
    let mut defaults = Toolbox::installed().defaults();
    defaults.retain(|name, _| name.starts_with("stand_in_"));
    assert_eq!(
        defaults,
        BTreeMap::from(
            [
                "stand_in_across",
                "stand_in_actions",
                "stand_in_change",
                "stand_in_read"
            ]
            .map(|name| (name.to_owned(), ToolLevel::Read))
        )
    );
    let reader = agent(&db, &[&north], &[]);
    let listed = db.agent_keys(&HashMap::new()).unwrap();
    let stand_ins: Vec<_> = listed
        .tools
        .iter()
        .filter(|tool| tool.name.starts_with("stand_in_"))
        .collect();
    let names: Vec<&str> = stand_ins.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "stand_in_read",
            "stand_in_change",
            "stand_in_actions",
            "stand_in_across"
        ]
    );
    assert_eq!(
        stand_ins
            .iter()
            .map(|tool| tool.changes)
            .collect::<Vec<_>>(),
        [false, true, true, false]
    );
    let request = |tools: &[(&str, &str)], all: bool| {
        let tools: BTreeMap<&str, &str> = tools.iter().copied().collect();
        AgentKeyRequest::parse(
            &json!({"name": reader.name, "allDsps": false, "dsps": [north],
            "access": "read", "allTools": all, "tools": tools, "expiresAt": null}),
        )
        .unwrap()
    };
    // Reading, a tool that changes something reads, and is refused what changes.
    let read_only = request(
        &[("stand_in_actions", "read"), ("stand_in_change", "read")],
        false,
    );
    db.update_agent_key(&owner, &reader.key, &read_only)
        .unwrap();
    let state = serving(db);
    let call = |name: &'static str, arguments: Value| {
        let (state, reader) = (state.clone(), reader.clone());
        async move { call_tool(&state, &reader, name, arguments).await }
    };
    assert!(
        call("stand_in_actions", json!({"action":"look"}))
            .await
            .is_ok()
    );
    let (code, message) = refusal(
        call(
            "stand_in_actions",
            json!({"action":"mark","note":"Checked"}),
        )
        .await,
    );
    assert_eq!(code, "not_allowed");
    assert!(
        message.contains("may only read with stand_in_actions"),
        "{message}"
    );
    assert_eq!(
        refusal(call("stand_in_change", json!({})).await).0,
        "not_allowed"
    );
    // Off, a tool is refused whatever it does; the tools about the connection always answer.
    assert_eq!(
        refusal(call("stand_in_read", json!({})).await).0,
        "not_allowed"
    );
    assert!(call("whoami", json!({})).await.is_ok());
    let at = north.clone();
    assert!(
        state
            .read(move |db| Ok(audit_actions(db, &at, "stand_in.")))
            .await
            .unwrap()
            .is_empty()
    );

    // Allowed to change, it changes on the next call, as the platform owner, by the agent.
    let changing = request(&[("stand_in_actions", "change")], false);
    let (owner_id, key) = (owner.clone(), reader.key.clone());
    state
        .run(move |db| db.update_agent_key(&owner_id, &key, &changing).map(|_| ()))
        .await
        .unwrap();
    assert!(
        call(
            "stand_in_actions",
            json!({"action":"mark","note":"Checked"})
        )
        .await
        .is_ok()
    );
    let events = state.read(|db| audits(db, None)).await.unwrap();
    let marked = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["action"] == "stand_in.marked")
        .unwrap();
    assert_eq!(marked["detail"], "Checked");
    assert_eq!(marked["changes"][0]["field"], "via");
    let updated = events
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["action"] == "agent.key_updated")
        .unwrap();
    assert_eq!(
        updated["changes"],
        json!([{"field":"tools","from":"[stand_in_actions, stand_in_change]",
            "to":"[stand_in_actions (changes)]"}])
    );
    // A call that says it only reads never writes, whatever its key may do.
    match call("stand_in_actions", json!({"action":"sneak"})).await {
        Err(Failure::Failed(error)) => assert_eq!(error.code, "tool_wrote_while_reading"),
        other => panic!("a write the call didn't declare: {other:?}"),
    }
    let marks = state
        .read(move |db| Ok(audit_actions(db, &north, "stand_in.")))
        .await
        .unwrap();
    assert_eq!(marks, ["stand_in.marked"]);
}

#[tokio::test]
async fn a_tool_added_since_reads_only_when_the_key_takes_new_tools() {
    crate::testing::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    let caller = agent(&db, &[&north], &[("stand_in_change", ToolLevel::Change)]);
    // Added since its tools were chosen, no choice names it.
    db.platform
        .exec("DELETE FROM agent_key_tools WHERE key_id=?", [&caller.key])
        .unwrap();
    let grants = crate::keys::agent_key_grants(&db, &caller.key, true).unwrap();
    for tool in Toolbox::installed().switchable() {
        assert_eq!(grants.level(tool), ToolLevel::Read, "{}", tool.name());
    }
    let mut request = AgentKeyRequest::parse(&json!({"name": caller.name, "allDsps": false,
        "dsps": [north], "access": "read", "allTools": false, "tools": {}, "expiresAt": null}))
    .unwrap();
    db.update_agent_key(&owner, &caller.key, &request).unwrap();
    db.platform
        .exec("DELETE FROM agent_key_tools WHERE key_id=?", [&caller.key])
        .unwrap();
    let grants = crate::keys::agent_key_grants(&db, &caller.key, false).unwrap();
    assert_eq!(grants.level(&crate::testing::StandInRead), ToolLevel::Off);

    // Only a tool there is can be chosen, and changing only with one that can.
    request.tools = BTreeMap::from([("drive_truck".to_owned(), ToolLevel::Read)]);
    let refused = db.update_agent_key(&owner, &caller.key, &request);
    assert_eq!(refused.unwrap_err().code, "invalid_input");
    request.tools = BTreeMap::from([("stand_in_read".to_owned(), ToolLevel::Change)]);
    let refused = db.update_agent_key(&owner, &caller.key, &request);
    assert_eq!(refused.unwrap_err().code, "invalid_input");

    // As the platform owners' emails word it, of the stand-ins.
    let toolbox = Toolbox::of(vec![
        &crate::testing::StandInRead,
        &crate::testing::StandInChange,
        &crate::testing::StandInActions,
        &crate::testing::StandInAcross,
    ]);
    let grants = |all: bool, chosen: &[(&str, ToolLevel)]| Grants {
        all,
        chosen: chosen
            .iter()
            .map(|(tool, level)| ((*tool).to_owned(), *level))
            .collect(),
    };
    assert_eq!(toolbox.summary(&grants(false, &[])), "None of 4");
    assert_eq!(
        toolbox.summary(&grants(true, &[])),
        "All 4; and new ones, to read"
    );
    assert_eq!(
        toolbox.summary(&grants(
            false,
            &[
                ("stand_in_read", ToolLevel::Read),
                ("stand_in_change", ToolLevel::Change),
                ("stand_in_across", ToolLevel::Off),
            ]
        )),
        "2 of 4, 1 that may make changes"
    );
}

#[tokio::test]
async fn the_connection_is_checked_again_before_a_tool_writes() {
    crate::testing::install();
    let (_root, db, north) = bootstrapped();
    let owner = platform_owner(&db);
    let caller = agent(&db, &[&north], &[("stand_in_change", ToolLevel::Change)]);
    let reached = db.find_dsp(&north).unwrap();
    let state = serving(db);
    let cx = Cx::new(
        state.clone(),
        caller.clone(),
        &crate::testing::StandInChange,
        Effect::Changes,
        vec![reached],
    );
    let (dsp, wrote) = (north.clone(), cx.clone());
    let write = move || {
        let (dsp, cx) = (dsp.clone(), wrote.clone());
        async move {
            cx.write(move |w| Ok(w.audit(&dsp, "stand_in.changed", "Changed", None, &[])?))
                .await
        }
    };
    assert!(write().await.is_ok());
    // Its key turned to reading only before the tool writes: the write is refused.
    let key = caller.key.clone();
    state
        .run(move |db| {
            let request = AgentKeyRequest::parse(&json!({"name": caller.name, "allDsps": false,
                "dsps": [north], "access": "read", "allTools": false,
                "tools": {"stand_in_change": "read"}, "expiresAt": null}))?;
            db.update_agent_key(&owner, &key, &request).map(|_| ())
        })
        .await
        .unwrap();
    assert_eq!(refusal(write().await).0, "not_allowed");
    // Revoked, it reads nothing either.
    let key = cx.caller().key.clone();
    let owner = state.read(|db| Ok(platform_owner(db))).await.unwrap();
    state
        .run(move |db| db.revoke_agent_key(&owner, &key).map(|_| ()))
        .await
        .unwrap();
    assert_eq!(refusal(cx.read(|_| Ok(())).await).0, "agent_key_revoked");
}

#[test]
fn a_reply_carries_its_data_words_and_pictures() {
    let image = Image {
        mime: ImageType::Png,
        bytes: vec![137, 80, 78, 71],
    };
    let answered = Reply::new(Said {
        said: "hello".into(),
    })
    .text("Said hello.")
    .image(image.clone())
    .answered()
    .unwrap();
    assert_eq!(
        answered,
        Answered {
            data: json!({"said":"hello"}),
            text: Some("Said hello.".into()),
            images: vec![image],
        }
    );
    let plain: Reply<Said> = Said { said: "hi".into() }.into();
    assert!(plain.text.is_none() && plain.images.is_empty());
}

struct Misnamed;
impl Tool for Misnamed {
    const NAME: &'static str = "Find Drivers";
    const TITLE: &'static str = "Find drivers";
    const DESCRIPTION: &'static str = "Find drivers.";
    type Input = Word;
    type Output = Said;
    async fn call(_: Cx, input: Word) -> Answer<Reply<Said>> {
        Ok(Said { said: input.word }.into())
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
    async fn call(_: Cx, input: Loose) -> Answer<Reply<Said>> {
        Ok(Said { said: input.word }.into())
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
enum LooseActions {
    Look { word: String },
}
struct LenientActions;
impl Tool for LenientActions {
    const NAME: &'static str = "lenient_actions";
    const TITLE: &'static str = "Lenient actions";
    const DESCRIPTION: &'static str = "Take any argument with an action.";
    type Input = LooseActions;
    type Output = Said;
    async fn call(_: Cx, input: LooseActions) -> Answer<Reply<Said>> {
        let LooseActions::Look { word } = input;
        Ok(Said { said: word }.into())
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
    async fn call(_: Cx, input: OwnDsp) -> Answer<Reply<Said>> {
        Ok(Said { said: input.dsp }.into())
    }
}
struct Unknown;
impl Tool for Unknown {
    const NAME: &'static str = "unknown";
    const TITLE: &'static str = "Unknown";
    const DESCRIPTION: &'static str = "Need a feature there isn't.";
    const FEATURES: &'static [&'static str] = &["teleport"];
    type Input = Nothing;
    type Output = Said;
    async fn call(_: Cx, _: Nothing) -> Answer<Reply<Said>> {
        Ok(Said { said: "".into() }.into())
    }
}
struct Meddler;
impl Tool for Meddler {
    const NAME: &'static str = "meddler";
    const TITLE: &'static str = "Meddler";
    const DESCRIPTION: &'static str = "Change the connection.";
    const SCOPE: Scope = Scope::Connection;
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = Said;
    async fn call(_: Cx, _: Nothing) -> Answer<Reply<Said>> {
        Ok(Said { said: "".into() }.into())
    }
}
struct Listing;
impl Tool for Listing {
    const NAME: &'static str = "listing";
    const TITLE: &'static str = "Listing";
    const DESCRIPTION: &'static str = "Answer with a list.";
    type Input = Nothing;
    type Output = Vec<String>;
    async fn call(_: Cx, _: Nothing) -> Answer<Reply<Vec<String>>> {
        Ok(vec![].into())
    }
}

#[test]
#[should_panic(expected = "is not lowercase words joined by _")]
fn a_tool_is_called_by_lowercase_words() {
    crate::testing::install();
    check(&[&Misnamed]);
}
#[test]
#[should_panic(expected = "two tools are called echo")]
fn two_tools_never_share_a_name() {
    crate::testing::install();
    check(&[&Echo, &Echo]);
}
#[test]
#[should_panic(expected = "lenient takes an object that refuses fields it doesn't name")]
fn a_tool_refuses_arguments_it_does_not_name() {
    crate::testing::install();
    check(&[&Lenient]);
}
#[test]
#[should_panic(expected = "lenient_actions takes an object that refuses fields it doesn't name")]
fn each_action_refuses_arguments_it_does_not_name() {
    crate::testing::install();
    check(&[&LenientActions]);
}
#[test]
#[should_panic(expected = "chooser names its own `dsp`")]
fn a_dsp_tool_leaves_choosing_the_dsp_to_the_server() {
    crate::testing::install();
    check(&[&Chooser]);
}
#[test]
#[should_panic(expected = "unknown needs teleport, which is no feature or part of one")]
fn a_tool_needs_only_features_there_are() {
    crate::testing::install();
    check(&[&Unknown]);
}
#[test]
#[should_panic(expected = "meddler is about the connection")]
fn a_tool_about_the_connection_only_reads() {
    crate::testing::install();
    check(&[&Meddler]);
}
#[test]
#[should_panic(expected = "listing answers something other than an object")]
fn a_tool_answers_an_object() {
    crate::testing::install();
    check(&[&Listing]);
}
#[test]
fn the_installed_tools_pass_the_checks() {
    crate::testing::install();
    check(Toolbox::installed().all());
}
