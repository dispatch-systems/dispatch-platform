//! What agents use: tools. Each is a type of its own, in a file of its own, that the feature
//! owning it lists in its manifest's `tools` (core lists its own, `connection`). A tool says
//! what it is: its name, what it does, what it takes and answers as Rust types whose JSON
//! Schema the server publishes, and whether it only reads or changes something. It never says
//! who may use it: that is the connection's (`Grants`), chosen when the platform owner adds the
//! agent and changed on the Agents page whenever they like.
//!
//! Before a tool runs, core checks the call, the same for every tool: the key or app still
//! stands and may use the tool, the DSP the call names is one it reaches, and that DSP has the
//! tool's feature switched on, hidden from its members or not. The tool is handed that DSP and
//! the store, under the shared lock when it reads and the exclusive one when it changes
//! something, and only answers; the server records the call in the Activity log.
mod connection;

use super::{Caller, api::types::AgentDsp};
use crate::{
    Error,
    accounts::api::types::Dsp,
    db::Store,
    manifest::{Feature, registry},
    mcp::api::types::AgentTool,
    tenancy::{audit::AuditChange, catalog},
};
use schemars::{JsonSchema, generate::SchemaSettings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::{
    any::type_name,
    collections::{BTreeSet, HashMap},
};

pub use connection::Nothing;
/// The schema crate the tools' types derive theirs with, so a feature uses the same one.
pub use schemars;

/// Whether a tool only reads, or changes something.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Reads,
    Changes,
}
/// What a tool is about: one DSP, which every call names unless the connection reaches only
/// one, or the connection itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Dsp,
    Connection,
}

/// A tool, as its owner declares it. `Input` and `Output` are its arguments and its answer;
/// both are JSON objects, and `Input` refuses fields it doesn't name
/// (`#[serde(deny_unknown_fields)]`), so an agent hears of a misspelled argument. A DSP tool
/// takes no `dsp` of its own: core reads it from the call and hands the tool the DSP.
pub trait Tool: Sync + 'static {
    /// Permanent: agents call the tool by it, and connections allow it by it. Lowercase words
    /// joined by `_`, unique among every tool.
    const NAME: &'static str;
    /// The tool's name as people read it.
    const TITLE: &'static str;
    /// What it does and answers, for the model choosing a tool: a sentence or two.
    const DESCRIPTION: &'static str;
    const EFFECT: Effect = Effect::Reads;
    const SCOPE: Scope = Scope::Dsp;
    /// One of its feature's parts it belongs to, when it isn't the feature as a whole: it is
    /// offered only where that part is on too.
    const PART: Option<&'static str> = None;
    type Input: DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;
    fn call(cx: &Cx, input: Self::Input) -> Answer<Self::Output>;
}

/// A tool as core holds it, whatever its types: what a manifest's `tools` lists, as
/// `tools: &[&ApproveTimecard]`.
pub trait AnyTool: Sync {
    fn name(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn effect(&self) -> Effect;
    fn scope(&self) -> Scope;
    fn part(&self) -> Option<&'static str>;
    /// The JSON Schema of what it takes, the `dsp` a DSP tool's call names included.
    fn input_schema(&self) -> Map<String, Value>;
    /// The JSON Schema of what it answers.
    fn output_schema(&self) -> Map<String, Value>;
    /// Reads the arguments, calls the tool and writes its answer.
    fn run(&self, cx: &Cx, arguments: Map<String, Value>) -> Answer<Value>;
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
    fn effect(&self) -> Effect {
        T::EFFECT
    }
    fn scope(&self) -> Scope {
        T::SCOPE
    }
    fn part(&self) -> Option<&'static str> {
        T::PART
    }
    fn input_schema(&self) -> Map<String, Value> {
        let mut schema = schema_of::<T::Input>();
        // Every tool lists what it takes, nothing included, as some hosts insist.
        let properties = schema.entry("properties").or_insert_with(|| json!({}));
        if T::SCOPE == Scope::Dsp
            && let Some(properties) = properties.as_object_mut()
        {
            // A tool's own `dsp` stays as it is, for the registry to refuse.
            properties
                .entry(DSP)
                .or_insert_with(|| json!({"type":"string","description":DSP_ABOUT}));
        }
        schema
    }
    fn output_schema(&self) -> Map<String, Value> {
        schema_of::<T::Output>()
    }
    fn run(&self, cx: &Cx, arguments: Map<String, Value>) -> Answer<Value> {
        let input = serde_json::from_value(Value::Object(arguments))
            .map_err(|error| Refusal::new("invalid_input", error.to_string()))?;
        let output = T::call(cx, input)?;
        serde_json::to_value(output).map_err(|_| Failure::Failed(Error::new("invalid_answer", 500)))
    }
}

/// The argument every DSP tool's call may name, and what it says.
const DSP: &str = "dsp";
const DSP_ABOUT: &str =
    "The DSP, by its name or ID. Leave it out when this connection reaches one DSP.";

/// The JSON Schema of a tool's arguments or answer, in the draft MCP names, without the
/// type's own title and description, which say nothing to a model.
fn schema_of<T: JsonSchema>() -> Map<String, Value> {
    let schema = SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    let Value::Object(mut schema) = serde_json::to_value(schema).expect("a schema") else {
        panic!("{} has no schema object", type_name::<T>());
    };
    schema.remove("title");
    schema.remove("description");
    schema
}

/// Why a tool gave no answer: something the agent can fix or ask the user about, or a
/// failure of ours.
#[derive(Debug)]
pub enum Failure {
    Refused(Refusal),
    Failed(Error),
}
/// A call the agent can fix, in words it can repeat: what was wrong, and what it could have
/// meant.
#[derive(Debug)]
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub choices: Vec<String>,
}
impl Refusal {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            choices: vec![],
        }
    }
    pub fn choices(mut self, choices: Vec<String>) -> Self {
        self.choices = choices;
        self
    }
}
impl From<Refusal> for Failure {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}
/// An error Dispatch refuses a request with, as a 404 or a 409, is one the agent can act on;
/// any other is a failure of ours.
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        if error.status < 500 {
            let message = "Dispatch refused the request.";
            Self::Refused(Refusal::new(&error.code, message))
        } else {
            Self::Failed(error)
        }
    }
}
/// A tool's answer, or why there is none.
pub type Answer<T> = std::result::Result<T, Failure>;

/// What a tool is handed for a call core checked: the store, the connection, and the DSP the
/// call is about.
pub struct Cx<'a> {
    store: &'a Store,
    caller: &'a Caller,
    dsp: Option<&'a Dsp>,
}
impl<'a> Cx<'a> {
    pub fn store(&self) -> &'a Store {
        self.store
    }
    /// The key or app calling, as it stands now.
    pub fn caller(&self) -> &'a Caller {
        self.caller
    }
    /// The DSP the call is about: one the connection reaches, with the tool's feature on. A
    /// connection tool has none.
    pub fn dsp(&self) -> &'a Dsp {
        self.dsp
            .expect("a DSP tool's call, which always names its DSP")
    }
    /// Records a change a tool made in the DSP's activity log, as made by the platform owner
    /// whose key or app made it, naming the agent.
    pub fn audit(
        &self,
        action: &str,
        detail: &str,
        target: Option<&str>,
        changes: &[AuditChange],
    ) -> crate::Result<()> {
        let mut changes = changes.to_vec();
        changes.push(("via", None, Some(self.caller.name.clone())));
        self.store.audit_with(
            Some(&self.caller.user),
            Some(&self.dsp().id),
            action,
            detail,
            target,
            &changes,
        )
    }
}

/// A tool with the feature that lists it; core's own have none, and are always on.
#[derive(Clone, Copy)]
pub struct Listed {
    pub tool: &'static dyn AnyTool,
    pub feature: Option<&'static Feature>,
}
impl Listed {
    /// The switch the tool is on with at a DSP: its part's, or its feature's.
    fn switch(self) -> Option<&'static str> {
        let feature = self.feature?;
        Some(self.tool.part().unwrap_or(feature.switch.id))
    }
}

/// The tools a server offers: core's, then each installed feature's.
pub struct Toolbox(Vec<Listed>);
impl Toolbox {
    pub fn installed() -> Self {
        let core = connection::TOOLS.iter().map(|&tool| Listed {
            tool,
            feature: None,
        });
        let features = registry().features.iter().flat_map(|&feature| {
            feature.tools.iter().map(move |&tool| Listed {
                tool,
                feature: Some(feature),
            })
        });
        Self::of(core.chain(features).collect())
    }
    pub fn of(tools: Vec<Listed>) -> Self {
        Self(tools)
    }
    pub fn all(&self) -> &[Listed] {
        &self.0
    }
    pub fn find(&self, name: &str) -> Option<Listed> {
        self.0
            .iter()
            .copied()
            .find(|listed| listed.tool.name() == name)
    }
    /// The tools a key or app can be allowed or not: every tool but core's about the
    /// connection itself, which are always allowed.
    pub fn switchable(&self) -> impl Iterator<Item = Listed> + '_ {
        self.0
            .iter()
            .copied()
            .filter(|listed| listed.tool.scope() == Scope::Dsp)
    }
    /// The switchable tools as the Agents page lists them.
    pub fn listed(&self) -> Vec<AgentTool> {
        self.switchable()
            .map(|listed| AgentTool {
                name: listed.tool.name().to_owned(),
                title: listed.tool.title().to_owned(),
                description: listed.tool.description().to_owned(),
                feature: listed
                    .feature
                    .map(|feature| feature.switch.label.to_owned()),
                changes: listed.tool.effect() == Effect::Changes,
            })
            .collect()
    }
    /// The switchable tools `grants` allow, by name.
    pub fn allowed(&self, grants: &Grants) -> Vec<String> {
        self.switchable()
            .filter(|listed| grants.allows(listed.tool))
            .map(|listed| listed.tool.name().to_owned())
            .collect()
    }
    /// What a new key or app starts with: every tool that only reads, and those added later.
    pub fn defaults(&self) -> Vec<String> {
        self.allowed(&Grants {
            all: true,
            chosen: HashMap::new(),
        })
    }
    /// What `grants` let a key or app use, in words, as its email says it.
    pub fn summary(&self, grants: &Grants) -> String {
        let total = self.switchable().count();
        let allowed: Vec<Listed> = self
            .switchable()
            .filter(|listed| grants.allows(listed.tool))
            .collect();
        let mut text = match (allowed.len(), total) {
            (_, 0) if grants.all => return "Any that only read, as they are added".into(),
            (_, 0) => return "None".into(),
            (0, _) => format!("None of {total}"),
            (count, _) if count == total => format!("All {total}"),
            (count, _) => format!("{count} of {total}"),
        };
        let changes = allowed
            .iter()
            .filter(|listed| listed.tool.effect() == Effect::Changes)
            .count();
        match changes {
            0 => {}
            1 => text.push_str(", 1 that makes changes"),
            _ => text.push_str(&format!(", {changes} that make changes")),
        }
        if grants.all {
            text.push_str("; and new ones that only read");
        }
        text
    }

    /// The tools a connection may use, each where at least one DSP it reaches has the tool's
    /// feature on. Any other is refused when called.
    pub fn offered(&self, db: &Store, caller: &Caller) -> crate::Result<Vec<Listed>> {
        let mut on = BTreeSet::new();
        for dsp in &caller.dsps {
            on.extend(db.features(&dsp.id)?);
        }
        Ok(self
            .0
            .iter()
            .copied()
            .filter(|listed| allowed(caller, listed.tool))
            .filter(|listed| match listed.tool.scope() {
                Scope::Connection => true,
                Scope::Dsp => listed.switch().is_none_or(|switch| on.contains(switch)),
            })
            .collect())
    }

    /// Calls a tool for a connection, once core has checked the call.
    pub fn invoke(
        &self,
        db: &Store,
        caller: &Caller,
        name: &str,
        mut arguments: Map<String, Value>,
    ) -> Called {
        let refused = |refusal: Refusal, dsp: Option<&Dsp>| Called {
            answer: Err(refusal.into()),
            dsp: dsp.map(about),
        };
        let Some(listed) = self.find(name) else {
            // A name is only the agent's own text: one too long to be a tool isn't repeated.
            let named = if name.len() <= 64 {
                format!("There is no tool `{name}`.")
            } else {
                "There is no such tool.".to_owned()
            };
            let names = match self.offered(db, caller) {
                Ok(offered) => offered.iter().map(|l| l.tool.name().to_owned()).collect(),
                Err(_) => vec![],
            };
            return refused(Refusal::new("unknown_tool", named).choices(names), None);
        };
        let tool = listed.tool;
        if !allowed(caller, tool) {
            let message = format!(
                "{} may not use {}. Tell the user, who can allow it on the Agents page in \
                 Dispatch.",
                caller.name,
                tool.name()
            );
            return refused(Refusal::new("not_allowed", message), None);
        }
        let dsp = match tool.scope() {
            Scope::Connection => None,
            Scope::Dsp => match pick(caller, arguments.remove(DSP)) {
                Ok(dsp) => Some(dsp),
                Err(refusal) => return refused(refusal, None),
            },
        };
        if let (Some(dsp), Some(switch)) = (dsp, listed.switch()) {
            match db.features(&dsp.id) {
                Ok(on) if on.iter().any(|id| id == switch) => {}
                Ok(_) => {
                    let label = catalog::find(switch).map_or(switch, |feature| feature.label);
                    let message = format!(
                        "{label} is switched off at {}. Tell the user; don't work the answer \
                         out from other tools.",
                        dsp.name
                    );
                    return refused(Refusal::new("switched_off", message), Some(dsp));
                }
                Err(error) => {
                    return Called {
                        answer: Err(error.into()),
                        dsp: Some(about(dsp)),
                    };
                }
            }
        }
        let cx = Cx {
            store: db,
            caller,
            dsp,
        };
        Called {
            answer: tool.run(&cx, arguments),
            dsp: dsp.map(about),
        }
    }
}

/// A tool's answer, with the DSP it was about, as the Activity log notes it.
pub struct Called {
    pub answer: Answer<Value>,
    pub dsp: Option<AgentDsp>,
}

fn about(dsp: &Dsp) -> AgentDsp {
    AgentDsp {
        id: dsp.id.clone(),
        name: dsp.name.clone(),
    }
}

/// Whether a connection may use a tool at all.
fn allowed(caller: &Caller, tool: &dyn AnyTool) -> bool {
    caller.tools.allows(tool)
}

/// What a key or app may use: each tool as the platform owner last chose it, and whether a tool
/// added since, which no choice names, is allowed when it only reads. One that changes
/// something waits until it is switched on. Core's tools about the connection itself are
/// always allowed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    pub all: bool,
    pub chosen: HashMap<String, bool>,
}
impl Grants {
    pub fn allows(&self, tool: &dyn AnyTool) -> bool {
        if tool.scope() == Scope::Connection {
            return true;
        }
        match self.chosen.get(tool.name()) {
            Some(&allowed) => allowed,
            None => self.all && tool.effect() == Effect::Reads,
        }
    }
}

/// The DSP a call names, by its ID or its name, among those the connection reaches; or the
/// only one it reaches when the call names none.
fn pick(caller: &Caller, named: Option<Value>) -> Result<&Dsp, Refusal> {
    let names = || caller.dsps.iter().map(|dsp| dsp.name.clone()).collect();
    let named = match named {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Some(_) => {
            let message = "`dsp` is the DSP's name or ID, as text.";
            return Err(Refusal::new("invalid_input", message).choices(names()));
        }
    };
    match (named, caller.dsps.as_slice()) {
        (None, [only]) => Ok(only),
        (None, []) => Err(Refusal::new(
            "no_dsp",
            "This connection reaches no DSP. Tell the user, who can give it one on the Agents \
             page in Dispatch.",
        )),
        (None, _) => Err(Refusal::new("dsp_required", "Name the DSP with `dsp`.").choices(names())),
        (Some(named), dsps) => dsps
            .iter()
            .find(|dsp| dsp.id == named || dsp.name.eq_ignore_ascii_case(&named))
            .ok_or_else(|| {
                let shown = if named.len() <= 80 {
                    format!("This connection reaches no DSP `{named}`.")
                } else {
                    "This connection reaches no such DSP.".to_owned()
                };
                Refusal::new("dsp_not_found", shown).choices(names())
            }),
    }
}

/// Panics unless every tool, core's and the features', has a name of its own as agents call
/// it, says what it is, belongs to a part of the feature listing it, refuses arguments it
/// doesn't name, leaves `dsp` to core, and answers an object.
pub fn check(features: &[&Feature]) {
    let core = connection::TOOLS.iter().map(|&tool| (tool, None));
    let theirs = features
        .iter()
        .flat_map(|&feature| feature.tools.iter().map(move |&tool| (tool, Some(feature))));
    let mut names = BTreeSet::new();
    for (tool, feature) in core.chain(theirs) {
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
        if let Some(part) = tool.part() {
            assert!(
                feature.is_some_and(|f| f.subfeatures.iter().any(|sub| sub.id == part)),
                "{name} belongs to {part}, which is no part of the feature listing it"
            );
        }
        let input = schema_of_input(tool);
        assert!(
            input["type"] == "object" && input["additionalProperties"] == false,
            "{name} takes an object that refuses fields it doesn't name"
        );
        assert!(
            tool.scope() == Scope::Connection
                || input["properties"][DSP] == json!({"type":"string","description":DSP_ABOUT}),
            "{name} names its own `dsp`, which core reads for a DSP tool"
        );
        assert_eq!(
            tool.output_schema()["type"],
            "object",
            "{name} answers something other than an object"
        );
    }
}
/// A tool's input schema, as published.
fn schema_of_input(tool: &dyn AnyTool) -> Value {
    Value::Object(tool.input_schema())
}

#[cfg(test)]
#[path = "../../tests/backend/tools/mod.rs"]
mod tests;
