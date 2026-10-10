//! The stable profile the agent is signed in as, which hosts such as ChatGPT ask for to tell
//! one signed-in account from another.
use super::Nothing;
use crate::{
    KeyStore,
    api::types::AgentProfile,
    toolbox::{Answer, Cx, Reply, Scope, Tool},
};

pub struct GetProfile;
impl Tool for GetProfile {
    const NAME: &'static str = "get_profile";
    const TITLE: &'static str = "Current Dispatch profile";
    const DESCRIPTION: &'static str = "Return the stable Dispatch profile represented by this request's authenticated credentials.";
    const SCOPE: Scope = Scope::Connection;
    type Input = Nothing;
    type Output = AgentProfile;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<AgentProfile>> {
        let caller = cx.caller().clone();
        cx.read(move |db| Ok(db.agent_profile(&caller)?.into()))
            .await
    }
}

#[cfg(test)]
#[path = "../tests/backend/tools/get_profile.rs"]
mod tests;
