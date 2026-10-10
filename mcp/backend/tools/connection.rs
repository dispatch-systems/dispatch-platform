//! Core's own tools, about the connection itself rather than a DSP: who the agent is signed in
//! as, and what its key or app reaches.
use super::{Answer, AnyTool, Cx, Scope, Tool};
use crate::KeyStore;
use crate::api::types::{AgentProfile, AgentWhoami};
use serde::Deserialize;

pub(super) const TOOLS: &[&dyn AnyTool] = &[&Profile, &Whoami];

/// What a tool that takes nothing takes.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Nothing {}

/// The stable profile the agent is signed in as, which hosts such as ChatGPT ask for to tell
/// one signed-in account from another.
pub struct Profile;
impl Tool for Profile {
    const NAME: &'static str = "get_profile";
    const TITLE: &'static str = "Current Dispatch profile";
    const DESCRIPTION: &'static str = "Return the stable Dispatch profile represented by this request's authenticated credentials.";
    const SCOPE: Scope = Scope::Connection;
    type Input = Nothing;
    type Output = AgentProfile;
    fn call(cx: &Cx, _: Nothing) -> Answer<AgentProfile> {
        Ok(cx.store().agent_profile(cx.caller())?)
    }
}

/// The connection's name, access and expiry, the time now, and each DSP it reaches with the
/// DSP's own date today.
pub struct Whoami;
impl Tool for Whoami {
    const NAME: &'static str = "whoami";
    const TITLE: &'static str = "This connection";
    const DESCRIPTION: &'static str = "Return this connection's name, access and expiry, the \
        time now, and each DSP it reaches with the DSP's own date today.";
    const SCOPE: Scope = Scope::Connection;
    type Input = Nothing;
    type Output = AgentWhoami;
    fn call(cx: &Cx, _: Nothing) -> Answer<AgentWhoami> {
        Ok(cx.store().agent_whoami(cx.caller())?)
    }
}
