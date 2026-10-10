//! Every tool agents use, a file each, listed in `TOOLS`. A tool is a type implementing
//! `toolbox::Tool`; `dispatchdev new tool <name>` makes one that works as written, with its
//! test, and lists it here. The connection, never the tool, says who may use it.
mod get_profile;
mod whoami;

use crate::toolbox::AnyTool;
use serde::Deserialize;

/// Every tool, in the order the Agents page lists them.
pub const TOOLS: &[&dyn AnyTool] = &[&get_profile::GetProfile, &whoami::Whoami];

/// What a tool that takes nothing takes.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nothing {}
