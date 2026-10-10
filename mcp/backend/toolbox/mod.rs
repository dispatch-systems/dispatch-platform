//! What tools are made of, and how the server offers and calls them. Every tool lives in
//! `mcp/tools/`, a type of its own in a file of its own, listed in `tools::TOOLS`. A tool says
//! what it is: its name, what it does, what it takes and answers as Rust types whose JSON
//! Schema the server publishes, the features it needs, whether it is about one DSP, several or
//! the connection itself, and what each call does, reading or changing something. One tool can
//! do several things: its input can be a choice of actions, each with its own arguments, and
//! it says which of them change something.
//!
//! A tool never says who may use it: that is the connection's (`Grants`), chosen per tool as
//! Off, Read, or Read and change when the platform owner adds the agent, and changed on the
//! Agents page whenever they like. Before a tool runs, the server checks the call, the same
//! for every tool: the key or app still stands and may do what the call does with the tool,
//! the DSPs it is about are ones the connection reaches, and they have the tool's features
//! switched on, hidden from their members or not. The tool then runs as async code with its
//! context (`Cx`): it reads with `cx.read` and changes something with `cx.write`, and may wait
//! or call another service in between. The connection is checked again before every read and
//! every write, as it stands then. It answers with its data, and words and pictures beside it
//! where they help (`Reply`); the server records the call in the Activity log.
mod cx;
mod grants;
mod reply;
mod schema;

pub use cx::{Cx, Writing};
pub use grants::Grants;
pub use reply::{Answer, Answered, Failure, Image, ImageType, Refusal, Reply};
/// The schema crate the tools' types derive theirs with.
pub use schemars;

use super::{Caller, api::types::AgentDsp};
use crate::api::types::{AgentTool, ToolLevel};
use dispatch_core::{
    State, accounts::api::types::Dsp, db::Store, manifest::registry, tenancy::catalog,
};
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

/// What a call does: only reads, or changes something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    Reads,
    Changes,
}
/// What a tool is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// One DSP, which a call names with `dsp`, or leaves out when the connection reaches one.
    Dsp,
    /// Several DSPs: those a call names with `dsps`, or when it names none, every one the
    /// connection reaches with the tool's features on.
    Dsps,
    /// The connection itself, at no DSP.
    Connection,
}

/// A tool, as it declares itself. `Input` and `Output` are its arguments and its answer;
/// both are JSON objects, and `Input` refuses fields it doesn't name
/// (`#[serde(deny_unknown_fields)]`), so an agent hears of a misspelled argument. `Input` may
/// be a choice of actions (`#[serde(tag = "action")]` on an enum), each with its own
/// arguments. A tool about DSPs takes no `dsp` or `dsps` of its own: the server reads them from
/// the call and hands the tool the DSPs.
pub trait Tool: Sync + 'static {
    /// Permanent: agents call the tool by it, and connections are granted it by it. Lowercase
    /// words joined by `_`, unique among every tool.
    const NAME: &'static str;
    /// The tool's name as people read it.
    const TITLE: &'static str;
    /// What it does and answers, for the model choosing a tool: a sentence or two.
    const DESCRIPTION: &'static str;
    const SCOPE: Scope = Scope::Dsp;
    /// The features it needs, each by its switch or a part's (`"timecard"`,
    /// `"timecard.meals"`): it runs at a DSP only where every one is on. One it uses only
    /// where it is on, it asks about with `cx.has`.
    const FEATURES: &'static [&'static str] = &[];
    /// The most it does: `Changes` when any call can change something.
    const EFFECT: Effect = Effect::Reads;
    type Input: DeserializeOwned + JsonSchema + Send + 'static;
    type Output: Serialize + JsonSchema + Send + 'static;
    /// What a call with `input` does. A tool of several actions answers for each; one that
    /// always does the same needn't.
    fn effect(_input: &Self::Input) -> Effect {
        Self::EFFECT
    }
    /// Answers a call the server has checked.
    fn call(cx: Cx, input: Self::Input)
    -> impl Future<Output = Answer<Reply<Self::Output>>> + Send;
}

/// A call's future, as the server awaits it.
pub type Running = Pin<Box<dyn Future<Output = Answer<Answered>> + Send>>;
/// A call's arguments, read as its tool takes them.
pub struct Input {
    effect: Effect,
    value: Box<dyn Any + Send>,
}

/// A tool as the server holds it, whatever its types: what `tools::TOOLS` lists.
pub trait AnyTool: Sync {
    fn name(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn scope(&self) -> Scope;
    fn features(&self) -> &'static [&'static str];
    /// The most it does.
    fn effect(&self) -> Effect;
    /// The JSON Schema of what it takes, the `dsp` or `dsps` a call about DSPs names
    /// included.
    fn input_schema(&self) -> Map<String, Value>;
    /// The JSON Schema of what it answers.
    fn output_schema(&self) -> Map<String, Value>;
    /// Reads a call's arguments as the tool takes them, with what the call does.
    fn read(&self, arguments: Map<String, Value>) -> Result<Input, Refusal>;
    /// Calls the tool with arguments it has read, and writes its answer.
    fn run(&self, cx: Cx, input: Input) -> Running;
}
impl<T: Tool> AnyTool for T {
    fn name(&self) -> &'static str {
        T::NAME
    }
    fn title(&self) -> &'static str {
        T::TITLE
    }
    fn description(&self) -> &'static str {
        T::DESCRIPTION
    }
    fn scope(&self) -> Scope {
        T::SCOPE
    }
    fn features(&self) -> &'static [&'static str] {
        T::FEATURES
    }
    fn effect(&self) -> Effect {
        T::EFFECT
    }
    fn input_schema(&self) -> Map<String, Value> {
        schema::input::<T::Input>(T::SCOPE)
    }
    fn output_schema(&self) -> Map<String, Value> {
        schema::of::<T::Output>()
    }
    fn read(&self, arguments: Map<String, Value>) -> Result<Input, Refusal> {
        let input: T::Input = serde_json::from_value(Value::Object(arguments))
            .map_err(|error| Refusal::new("invalid_input", error.to_string()))?;
        Ok(Input {
            effect: T::effect(&input),
            value: Box::new(input),
        })
    }
    fn run(&self, cx: Cx, input: Input) -> Running {
        let input = *input
            .value
            .downcast::<T::Input>()
            .expect("arguments read by the same tool");
        Box::pin(async move { T::call(cx, input).await?.answered() })
    }
}

/// The arguments a call about DSPs names them with, and what each says.
const DSP: &str = "dsp";
const DSP_ABOUT: &str =
    "The DSP, by its name or ID. Leave it out when this connection reaches one DSP.";
const DSPS: &str = "dsps";
const DSPS_ABOUT: &str = "The DSPs, each by its name or ID. Leave it out for every DSP this \
    connection reaches with the tool's features on.";

/// The tools a server offers: `tools::TOOLS`, and any the installed MCP adds, as a test's;
/// each but one that needs a feature this build leaves out.
pub struct Toolbox(Vec<&'static dyn AnyTool>);
impl Toolbox {
    pub fn installed() -> Self {
        let built = built();
        let tools = crate::tools::TOOLS.iter().chain(super::piece::own().tools);
        Self(
            tools
                .copied()
                .filter(|tool| tool.features().iter().all(|switch| built.contains(switch)))
                .collect(),
        )
    }
    pub fn of(tools: Vec<&'static dyn AnyTool>) -> Self {
        Self(tools)
    }
    pub fn all(&self) -> &[&'static dyn AnyTool] {
        &self.0
    }
    pub fn find(&self, name: &str) -> Option<&'static dyn AnyTool> {
        self.0.iter().copied().find(|tool| tool.name() == name)
    }
    /// The tools a key or app is granted or not: every tool but those about the connection
    /// itself, which every connection may use.
    pub fn switchable(&self) -> impl Iterator<Item = &'static dyn AnyTool> + '_ {
        self.0
            .iter()
            .copied()
            .filter(|tool| tool.scope() != Scope::Connection)
    }
    /// The switchable tools as the Agents page lists them.
    pub fn listed(&self) -> Vec<AgentTool> {
        self.switchable()
            .map(|tool| AgentTool {
                name: tool.name().to_owned(),
                title: tool.title().to_owned(),
                description: tool.description().to_owned(),
                features: tool.features().iter().map(|switch| named(switch)).collect(),
                changes: tool.effect() == Effect::Changes,
            })
            .collect()
    }
    /// What `grants` let a key or app do with each switchable tool it may use, by name.
    pub fn granted(&self, grants: &Grants) -> BTreeMap<String, ToolLevel> {
        self.switchable()
            .map(|tool| (tool.name().to_owned(), grants.level(tool)))
            .filter(|(_, level)| *level != ToolLevel::Off)
            .collect()
    }
    /// What a new key or app starts with: every tool, to read, and those added later.
    pub fn defaults(&self) -> BTreeMap<String, ToolLevel> {
        self.granted(&Grants {
            all: true,
            chosen: Default::default(),
        })
    }
    /// What `grants` let a key or app use, in words, as its email says it.
    pub fn summary(&self, grants: &Grants) -> String {
        let total = self.switchable().count();
        let levels: Vec<ToolLevel> = self
            .switchable()
            .map(|tool| grants.level(tool))
            .filter(|level| *level != ToolLevel::Off)
            .collect();
        let mut text = match (levels.len(), total) {
            (_, 0) if grants.all => return "Any, to read, as they are added".into(),
            (_, 0) => return "None".into(),
            (0, _) => format!("None of {total}"),
            (count, _) if count == total => format!("All {total}"),
            (count, _) => format!("{count} of {total}"),
        };
        match levels
            .iter()
            .filter(|level| **level == ToolLevel::Change)
            .count()
        {
            0 => {}
            1 => text.push_str(", 1 that may make changes"),
            changes => text.push_str(&format!(", {changes} that may make changes")),
        }
        if grants.all {
            text.push_str("; and new ones, to read");
        }
        text
    }

    /// The tools a connection may use, each where at least one DSP it reaches has every
    /// feature the tool needs on. Any other is refused when called.
    pub fn offered(&self, db: &Store, caller: &Caller) -> dispatch_core::Result<Vec<Offered>> {
        let mut on = Vec::new();
        for dsp in &caller.dsps {
            on.push(db.features(&dsp.id)?);
        }
        Ok(self
            .0
            .iter()
            .copied()
            .filter(|tool| caller.tools.level(*tool) != ToolLevel::Off)
            .filter(|tool| {
                tool.scope() == Scope::Connection
                    || on.iter().any(|switches| has_all(switches, tool.features()))
            })
            .map(|tool| Offered {
                tool,
                level: caller.tools.level(tool),
            })
            .collect())
    }

    /// Calls a tool for a connection as it stands now: the call checked, then the tool run
    /// with its context.
    pub async fn call(
        &self,
        state: Arc<State>,
        caller: Caller,
        name: String,
        arguments: Map<String, Value>,
    ) -> Called {
        let Some(tool) = self.find(&name) else {
            // A name is only the agent's own text: one too long to be a tool isn't repeated.
            let named = if name.len() <= 64 {
                format!("There is no tool `{name}`.")
            } else {
                "There is no such tool.".to_owned()
            };
            let tools = Toolbox(self.0.clone());
            let offered = state
                .read(move |db| {
                    let current = db_caller(db, &caller)?;
                    let offered = tools.offered(db, &current)?;
                    Ok(offered.iter().map(|o| o.tool.name().to_owned()).collect())
                })
                .await;
            return match offered {
                Ok(offered) => {
                    Called::refused(Refusal::new("unknown_tool", named).choices(offered))
                }
                Err(error) => Called::failed(error.into(), vec![]),
            };
        };
        let mut arguments = arguments;
        let named = match tool.scope() {
            Scope::Dsp => Named::One(arguments.remove(DSP)),
            Scope::Dsps => Named::Several(arguments.remove(DSPS)),
            Scope::Connection => Named::None,
        };
        let read = tool.read(arguments);
        let checked = state
            .read(move |db| Ok(check_call(db, &caller, tool, read, named)))
            .await
            .unwrap_or_else(|error| Err((error.into(), vec![])));
        let Checked {
            caller,
            dsps,
            input,
        } = match checked {
            Ok(checked) => checked,
            Err((failure, dsps)) => return Called::failed(failure, dsps),
        };
        let about = noted(&dsps);
        let cx = Cx::new(state, caller, tool, input.effect, dsps);
        Called {
            answer: tool.run(cx, input).await,
            dsp: about,
        }
    }
}

/// A tool a connection may use, with what it may do with it.
#[derive(Clone, Copy)]
pub struct Offered {
    pub tool: &'static dyn AnyTool,
    pub level: ToolLevel,
}

/// A tool's answer, with the DSP it was about, as the Activity log notes it.
pub struct Called {
    pub answer: Answer<Answered>,
    pub dsp: Option<AgentDsp>,
}
impl Called {
    fn refused(refusal: Refusal) -> Self {
        Self::failed(refusal.into(), vec![])
    }
    fn failed(failure: Failure, dsps: Vec<Dsp>) -> Self {
        Self {
            answer: Err(failure),
            dsp: noted(&dsps),
        }
    }
}
/// The DSP a call was about, as the Activity log notes it: the one, when it was about one.
fn noted(dsps: &[Dsp]) -> Option<AgentDsp> {
    match dsps {
        [dsp] => Some(AgentDsp {
            id: dsp.id.clone(),
            name: dsp.name.clone(),
        }),
        _ => None,
    }
}

/// The DSPs a call named, as it came.
enum Named {
    One(Option<Value>),
    Several(Option<Value>),
    None,
}

/// The connection as it stands now.
fn db_caller(db: &Store, caller: &Caller) -> dispatch_core::Result<Caller> {
    use crate::KeyStore;
    db.revalidate_agent(caller)
}

/// A call checked: the connection as it stands now, the DSPs the call is about, and its
/// arguments as the tool takes them.
struct Checked {
    caller: Caller,
    dsps: Vec<Dsp>,
    input: Input,
}
/// Why a checked call was refused, with the DSPs it was about when one was named.
type Unchecked = (Failure, Vec<Dsp>);

/// Checks a call before its tool runs, in the order an agent can act on: the key or app
/// still stands as it is now; may use the tool at all; named arguments the tool takes; may do
/// what the call does with the tool; and names DSPs it reaches with the tool's features on.
fn check_call(
    db: &Store,
    caller: &Caller,
    tool: &dyn AnyTool,
    read: Result<Input, Refusal>,
    named: Named,
) -> Result<Checked, Unchecked> {
    let none = |failure: Failure| (failure, vec![]);
    let current = db_caller(db, caller).map_err(|error| none(error.into()))?;
    if current.tools.level(tool) == ToolLevel::Off {
        return Err(none(refused(&current, tool).into()));
    }
    let input = read.map_err(|refusal| none(refusal.into()))?;
    permitted(&current, tool, input.effect).map_err(|refusal| none(refusal.into()))?;
    let dsps = match named {
        Named::None => vec![],
        Named::One(named) => {
            let dsp = pick(&current, named)
                .map_err(|refusal| none(refusal.into()))?
                .clone();
            if let Err(failure) = switched_on(db, tool, &dsp) {
                return Err((failure, vec![dsp]));
            }
            vec![dsp]
        }
        Named::Several(named) => pick_several(db, &current, tool, named).map_err(none)?,
    };
    Ok(Checked {
        caller: current,
        dsps,
        input,
    })
}

/// Whether a connection may do what a call does with a tool, and if not, why.
fn permitted(caller: &Caller, tool: &dyn AnyTool, effect: Effect) -> Result<(), Refusal> {
    if caller.tools.allows(tool, effect) {
        Ok(())
    } else {
        Err(refused(caller, tool))
    }
}
/// Why a connection may not make a call to a tool: it may not use it, or may only read with it.
fn refused(caller: &Caller, tool: &dyn AnyTool) -> Refusal {
    let message = if caller.tools.level(tool) == ToolLevel::Off {
        format!(
            "{} may not use {}. Tell the user, who can allow it on the Agents page in Dispatch.",
            caller.name,
            tool.name()
        )
    } else {
        format!(
            "{} may only read with {}, and this changes something. Tell the user, who can \
             allow it to make changes on the Agents page in Dispatch.",
            caller.name,
            tool.name()
        )
    };
    Refusal::new("not_allowed", message)
}

/// Refuses a DSP that lacks a feature the tool needs, hidden from its members or not.
fn switched_on(db: &Store, tool: &dyn AnyTool, dsp: &Dsp) -> Answer<()> {
    let on = db.features(&dsp.id)?;
    match tool
        .features()
        .iter()
        .find(|switch| !on.iter().any(|id| id == *switch))
    {
        None => Ok(()),
        Some(off) => {
            let message = format!(
                "{} is switched off at {}. Tell the user; don't work the answer out from other \
                 tools.",
                label(off),
                dsp.name
            );
            Err(Refusal::new("switched_off", message).into())
        }
    }
}

/// Whether every feature of `needed` is among `on`.
fn has_all(on: &[String], needed: &[&str]) -> bool {
    needed.iter().all(|switch| on.iter().any(|id| id == switch))
}
/// A feature, or its part, as the DSPs page names it.
fn label(switch: &str) -> &str {
    catalog::find(switch).map_or(switch, |feature| feature.label)
}
/// A feature, or a part with its feature, as the Agents page groups tools under it:
/// `Timecard · Meal breaks`.
fn named(switch: &str) -> String {
    match switch.split_once('.') {
        Some((feature, _)) => format!("{} · {}", label(feature), label(switch)),
        None => label(switch).to_owned(),
    }
}

/// The DSP a call names, by its ID or its name, among those the connection reaches; or the
/// only one it reaches when the call names none.
fn pick(caller: &Caller, named: Option<Value>) -> Result<&Dsp, Refusal> {
    let named = match named {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Some(_) => {
            let message = "`dsp` is the DSP's name or ID, as text.";
            return Err(Refusal::new("invalid_input", message).choices(names(caller)));
        }
    };
    match (named, caller.dsps.as_slice()) {
        (None, [only]) => Ok(only),
        (None, []) => Err(no_dsp()),
        (None, _) => {
            Err(Refusal::new("dsp_required", "Name the DSP with `dsp`.").choices(names(caller)))
        }
        (Some(named), _) => find(caller, &named),
    }
}

/// The DSPs a call about several names, each among those the connection reaches with the
/// tool's features on; or when it names none, every one it reaches with them on.
fn pick_several(
    db: &Store,
    caller: &Caller,
    tool: &dyn AnyTool,
    named: Option<Value>,
) -> Answer<Vec<Dsp>> {
    let named: Vec<String> = match named {
        None | Some(Value::Null) => vec![],
        Some(Value::Array(items)) if items.len() <= 500 => {
            let mut named = Vec::new();
            for item in items {
                match item {
                    Value::String(text) if !text.trim().is_empty() => {
                        named.push(text.trim().to_owned());
                    }
                    _ => return Err(several_refused(caller).into()),
                }
            }
            named
        }
        Some(_) => return Err(several_refused(caller).into()),
    };
    if caller.dsps.is_empty() {
        return Err(no_dsp().into());
    }
    if !named.is_empty() {
        let mut dsps: Vec<Dsp> = Vec::new();
        for named in &named {
            let dsp = find(caller, named)?;
            if !dsps.iter().any(|picked| picked.id == dsp.id) {
                switched_on(db, tool, dsp)?;
                dsps.push(dsp.clone());
            }
        }
        return Ok(dsps);
    }
    let mut dsps = Vec::new();
    for dsp in &caller.dsps {
        if has_all(&db.features(&dsp.id)?, tool.features()) {
            dsps.push(dsp.clone());
        }
    }
    if dsps.is_empty() {
        let needed: Vec<&str> = tool.features().iter().map(|switch| label(switch)).collect();
        let message = format!(
            "{} is switched off at every DSP this connection reaches. Tell the user; don't \
             work the answer out from other tools.",
            needed.join(" or ")
        );
        return Err(Refusal::new("switched_off", message).into());
    }
    Ok(dsps)
}
fn several_refused(caller: &Caller) -> Refusal {
    let message = "`dsps` is a list of DSPs, each by its name or ID, as text.";
    Refusal::new("invalid_input", message).choices(names(caller))
}
fn no_dsp() -> Refusal {
    Refusal::new(
        "no_dsp",
        "This connection reaches no DSP. Tell the user, who can give it one on the Agents page \
         in Dispatch.",
    )
}
fn names(caller: &Caller) -> Vec<String> {
    caller.dsps.iter().map(|dsp| dsp.name.clone()).collect()
}
/// A DSP the connection reaches, by its ID or its name.
fn find<'a>(caller: &'a Caller, named: &str) -> Result<&'a Dsp, Refusal> {
    caller
        .dsps
        .iter()
        .find(|dsp| dsp.id == named || dsp.name.eq_ignore_ascii_case(named))
        .ok_or_else(|| {
            let shown = if named.len() <= 80 {
                format!("This connection reaches no DSP `{named}`.")
            } else {
                "This connection reaches no such DSP.".to_owned()
            };
            Refusal::new("dsp_not_found", shown).choices(names(caller))
        })
}

/// Every switch this build has: each feature's, and each of its parts'.
fn built() -> BTreeSet<&'static str> {
    registry()
        .features
        .iter()
        .flat_map(|feature| {
            std::iter::once(feature.switch.id).chain(feature.subfeatures.iter().map(|sub| sub.id))
        })
        .collect()
}

/// Panics unless every tool has a name of its own as agents call it, says what it is, needs
/// only features there are, refuses arguments it doesn't name, leaves `dsp` and `dsps` to the
/// server, and answers an object.
pub fn check(tools: &[&dyn AnyTool]) {
    let switches = built();
    let mut names = BTreeSet::new();
    for tool in tools {
        let name = tool.name();
        assert!(
            !name.is_empty()
                && name.len() <= 64
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "the tool {name:?} is not lowercase words joined by _"
        );
        assert!(names.insert(name), "two tools are called {name}");
        assert!(
            !tool.title().is_empty() && !tool.description().is_empty(),
            "{name} has no title or description"
        );
        for switch in tool.features() {
            assert!(
                switches.contains(switch),
                "{name} needs {switch}, which is no feature or part of one"
            );
        }
        assert!(
            tool.scope() != Scope::Connection || tool.features().is_empty(),
            "{name} is about the connection, at no DSP, so it needs no feature"
        );
        assert!(
            tool.scope() != Scope::Connection || tool.effect() == Effect::Reads,
            "{name} is about the connection, which every connection may use, so it only reads"
        );
        schema::check(
            name,
            tool.scope(),
            &tool.input_schema(),
            &tool.output_schema(),
        );
    }
}

#[cfg(test)]
#[path = "../../tests/backend/toolbox/mod.rs"]
mod tests;
