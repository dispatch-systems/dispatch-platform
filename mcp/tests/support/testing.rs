//! The MCP's test support, for its own tests and, behind the `testing` feature, the app's: the
//! MCP with stand-in tools in core's test registry, with a stand-in for each feature a tool
//! needs, a key as an agent signs in with it, the server's state, and a tool called as the
//! server calls it.
use super::{
    Caller, KeyStore,
    api::types::{AgentAccess, AgentKeyRequest, ToolLevel},
    piece::{Own, piece},
    toolbox::{Answer, Answered, AnyTool, Cx, Effect, Reply, Scope, Tool, Toolbox},
    tools::{Nothing, TOOLS},
};
use dispatch_core::{
    State,
    db::Store,
    manifest::{Agents, Feature, feature, optional, sub},
    testing::{self, platform_owner},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

/// The stand-in tools, which its tests add to `tools::TOOLS`.
const STAND_INS: &[&dyn AnyTool] = &[
    &StandInRead,
    &StandInChange,
    &StandInActions,
    &StandInAcross,
];

/// The MCP with the stand-in tools, as its tests install it: one static, so its address is the
/// registry's own.
pub static AGENTS: Agents = piece(&Own { tools: STAND_INS });

/// Installs core's test registry with the MCP, `AGENTS`, as its agents' piece, and a stand-in
/// for each feature a tool needs.
pub fn install() {
    testing::install_with(Some(&AGENTS), &[], needed());
}

/// A stand-in for each feature a tool needs, with the parts the tools name: the MCP's tests
/// install none of the product's features, so a tool's test switches these on and off as it
/// would the feature itself.
fn needed() -> &'static [&'static Feature] {
    static NEEDED: OnceLock<&'static [&'static Feature]> = OnceLock::new();
    fn leak<T>(value: T) -> &'static T {
        Box::leak(Box::new(value))
    }
    // `timecard.meal_breaks` as people read it: Timecard meal breaks.
    fn label(id: &str) -> &'static str {
        let words = id.replace(['.', '_'], " ");
        let mut chars = words.chars();
        let label = chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).collect::<String>())
            .unwrap_or_default();
        Box::leak(label.into_boxed_str())
    }
    NEEDED.get_or_init(|| {
        let mut parts: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
        for tool in TOOLS.iter().chain(STAND_INS) {
            for &switch in tool.features() {
                let owner = switch.split('.').next().unwrap_or(switch);
                let named = parts.entry(owner).or_default();
                if switch != owner && !named.contains(&switch) {
                    named.push(switch);
                }
            }
        }
        let features: Vec<&'static Feature> = parts
            .into_iter()
            .map(|(name, parts)| {
                let subfeatures: Vec<_> = parts.into_iter().map(|id| sub(id, label(id))).collect();
                leak(Feature {
                    switch: optional(name, label(name), &[]),
                    subfeatures: Box::leak(subfeatures.into_boxed_slice()),
                    ..feature(name)
                })
            })
            .collect();
        Box::leak(features.into_boxed_slice())
    })
}

/// The server's state over the database `db` holds, as the server runs on it: `db` is set up
/// first, then handed over.
pub fn serving(db: Store) -> Arc<State> {
    let config = db.config.clone();
    drop(db);
    State::new(config).unwrap()
}

/// A key the platform owner made, reaching `dsps` (every DSP when none), granted `tools` as
/// each says and taking tools added later to read, as an agent signs in with it.
pub fn agent(db: &Store, dsps: &[&str], tools: &[(&str, ToolLevel)]) -> Caller {
    static MADE: AtomicU32 = AtomicU32::new(0);
    let made = MADE.fetch_add(1, Ordering::Relaxed);
    let tools: BTreeMap<&str, ToolLevel> = tools.iter().copied().collect();
    let request = AgentKeyRequest::parse(&json!({
        "name": format!("Test agent {made}"),
        "allDsps": dsps.is_empty(),
        "dsps": dsps,
        "access": AgentAccess::Read,
        "allTools": true,
        "tools": tools,
        "expiresAt": null,
    }))
    .unwrap();
    let key = db.create_agent_key(&platform_owner(db), &request).unwrap();
    db.authenticate_agent(&key.token, "test").unwrap()
}

/// Calls a tool of the installed registry as `caller`, checked as the MCP server checks a
/// call: `call_tool(&state, &caller, "inspections", json!({"dsp": dsp, "week": "…"}))`.
pub async fn call_tool(
    state: &Arc<State>,
    caller: &Caller,
    name: &str,
    arguments: Value,
) -> Answer<Answered> {
    let Value::Object(arguments) = arguments else {
        panic!("a tool's arguments are an object");
    };
    Toolbox::installed()
        .call(state.clone(), caller.clone(), name.to_owned(), arguments)
        .await
        .answer
}

/// What a stand-in tool answers: the DSPs it was handed.
#[derive(serde::Serialize, schemars::JsonSchema)]
pub struct StandInAnswer {
    pub dsps: Vec<String>,
}
fn handed(cx: &Cx) -> StandInAnswer {
    StandInAnswer {
        dsps: cx.dsps().iter().map(|dsp| dsp.name.clone()).collect(),
    }
}
/// A tool that reads, about one DSP.
pub struct StandInRead;
impl Tool for StandInRead {
    const NAME: &'static str = "stand_in_read";
    const TITLE: &'static str = "Stand-in read";
    const DESCRIPTION: &'static str = "Answer with the DSP.";
    type Input = Nothing;
    type Output = StandInAnswer;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<StandInAnswer>> {
        Ok(Reply::new(handed(&cx)).text("Read."))
    }
}
/// A tool that changes something at one DSP: it records that it did.
pub struct StandInChange;
impl Tool for StandInChange {
    const NAME: &'static str = "stand_in_change";
    const TITLE: &'static str = "Stand-in change";
    const DESCRIPTION: &'static str = "Record a change at the DSP.";
    const EFFECT: Effect = Effect::Changes;
    type Input = Nothing;
    type Output = StandInAnswer;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<StandInAnswer>> {
        let dsp = cx.dsp().id.clone();
        cx.write(move |w| Ok(w.audit(&dsp, "stand_in.changed", "Changed", None, &[])?))
            .await?;
        Ok(handed(&cx).into())
    }
}
/// What the stand-in of several actions is asked to do.
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StandInAction {
    /// Answer with the DSP.
    Look,
    /// Record a mark at the DSP.
    Mark {
        /// What the mark says.
        note: String,
    },
    /// Write while saying it only reads, as no tool may.
    Sneak,
}
/// A tool of several actions: one reads, one changes something, and one breaks its word.
pub struct StandInActions;
impl Tool for StandInActions {
    const NAME: &'static str = "stand_in_actions";
    const TITLE: &'static str = "Stand-in actions";
    const DESCRIPTION: &'static str = "Look at the DSP, or mark it.";
    const EFFECT: Effect = Effect::Changes;
    type Input = StandInAction;
    type Output = StandInAnswer;
    fn effect(input: &StandInAction) -> Effect {
        match input {
            StandInAction::Mark { .. } => Effect::Changes,
            StandInAction::Look | StandInAction::Sneak => Effect::Reads,
        }
    }
    async fn call(cx: Cx, input: StandInAction) -> Answer<Reply<StandInAnswer>> {
        let dsp = cx.dsp().id.clone();
        match input {
            StandInAction::Look => {}
            StandInAction::Mark { note } => {
                cx.write(move |w| Ok(w.audit(&dsp, "stand_in.marked", &note, None, &[])?))
                    .await?;
            }
            StandInAction::Sneak => {
                cx.write(move |w| Ok(w.audit(&dsp, "stand_in.marked", "Sneaked", None, &[])?))
                    .await?;
            }
        }
        Ok(handed(&cx).into())
    }
}
/// A tool about several DSPs: it reads each it was handed.
pub struct StandInAcross;
impl Tool for StandInAcross {
    const NAME: &'static str = "stand_in_across";
    const TITLE: &'static str = "Stand-in across";
    const DESCRIPTION: &'static str = "Answer with the DSPs.";
    const SCOPE: Scope = Scope::Dsps;
    type Input = Nothing;
    type Output = StandInAnswer;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<StandInAnswer>> {
        Ok(handed(&cx).into())
    }
}
