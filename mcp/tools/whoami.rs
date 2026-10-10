//! The connection's name, access and expiry, the time now, and each DSP it reaches with the
//! DSP's own date today.
use super::Nothing;
use crate::{
    KeyStore,
    api::types::AgentWhoami,
    toolbox::{Answer, Cx, Reply, Scope, Tool},
};

pub struct Whoami;
impl Tool for Whoami {
    const NAME: &'static str = "whoami";
    const TITLE: &'static str = "This connection";
    const DESCRIPTION: &'static str = "Return this connection's name, access and expiry, the \
        time now, and each DSP it reaches with the DSP's own date today.";
    const SCOPE: Scope = Scope::Connection;
    type Input = Nothing;
    type Output = AgentWhoami;
    async fn call(cx: Cx, _: Nothing) -> Answer<Reply<AgentWhoami>> {
        let caller = cx.caller().clone();
        cx.read(move |db| Ok(db.agent_whoami(&caller)?.into()))
            .await
    }
}
