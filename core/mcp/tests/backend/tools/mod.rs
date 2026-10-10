use super::*;
use crate::{
    foundation::config::Environment,
    manifest::{feature, optional},
    tenancy::api::types::DspStatus,
};
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
    crate::testing::install(&[], &[]);
    let (_root, db) = crate::testing::store();
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
    static LISTING: Feature = Feature {
        switch: optional("listing", "Listing", &[]),
        tools: &[&Misnamed],
        ..feature("listing")
    };
    check(&[&LISTING]);
}
#[test]
#[should_panic(expected = "two tools are called echo")]
fn two_tools_never_share_a_name() {
    static FIRST: Feature = Feature {
        switch: optional("first", "First", &[]),
        tools: &[&Echo],
        ..feature("first")
    };
    static SECOND: Feature = Feature {
        switch: optional("second", "Second", &[]),
        tools: &[&Echo],
        ..feature("second")
    };
    check(&[&FIRST, &SECOND]);
}
#[test]
#[should_panic(expected = "lenient takes an object that refuses fields it doesn't name")]
fn a_tool_refuses_arguments_it_does_not_name() {
    static LISTING: Feature = Feature {
        switch: optional("listing", "Listing", &[]),
        tools: &[&Lenient],
        ..feature("listing")
    };
    check(&[&LISTING]);
}
#[test]
#[should_panic(expected = "chooser names its own `dsp`")]
fn a_dsp_tool_leaves_choosing_the_dsp_to_core() {
    static LISTING: Feature = Feature {
        switch: optional("listing", "Listing", &[]),
        tools: &[&Chooser],
        ..feature("listing")
    };
    check(&[&LISTING]);
}
#[test]
#[should_panic(expected = "elsewhere belongs to timecard.meals, which is no part of the feature")]
fn a_tool_belongs_to_a_part_of_its_own_feature() {
    static LISTING: Feature = Feature {
        switch: optional("listing", "Listing", &[]),
        tools: &[&Elsewhere],
        ..feature("listing")
    };
    check(&[&LISTING]);
}
