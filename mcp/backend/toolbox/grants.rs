//! What a connection may do with each tool: the platform owner's choice, Off, Read, or Read
//! and change.
use super::{AnyTool, Effect, Scope};
use crate::api::types::ToolLevel;
use std::collections::HashMap;

/// What a key or app may do with each tool, as the platform owner last chose it, and whether
/// a tool added since, which no choice names, comes to read (`all`). Nothing added later
/// changes something until it is allowed to. The tools about the connection itself, which
/// only read, every connection may use.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    pub all: bool,
    pub chosen: HashMap<String, ToolLevel>,
}
impl Grants {
    /// What the connection may do with `tool`. A tool that only reads is never more than
    /// Read.
    pub fn level(&self, tool: &dyn AnyTool) -> ToolLevel {
        if tool.scope() == Scope::Connection {
            return ToolLevel::Read;
        }
        let level = match self.chosen.get(tool.name()) {
            Some(level) => *level,
            None if self.all => ToolLevel::Read,
            None => ToolLevel::Off,
        };
        match (level, tool.effect()) {
            (ToolLevel::Change, Effect::Reads) => ToolLevel::Read,
            (level, _) => level,
        }
    }
    /// Whether the connection may make a call to `tool` that does `effect`.
    pub fn allows(&self, tool: &dyn AnyTool, effect: Effect) -> bool {
        match self.level(tool) {
            ToolLevel::Off => false,
            ToolLevel::Read => effect == Effect::Reads,
            ToolLevel::Change => true,
        }
    }
}
