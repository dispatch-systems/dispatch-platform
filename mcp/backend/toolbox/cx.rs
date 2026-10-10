//! What a tool runs with: the connection and the DSPs a checked call is about, and the
//! database, which it reads and writes through its context so the connection is checked again
//! each time, as it stands then.
use super::{Answer, AnyTool, Effect, Failure, Refusal, db_caller, has_all, permitted};
use crate::Caller;
use dispatch_core::{
    Error, State, accounts::api::types::Dsp, db::Store, tenancy::audit::AuditChange,
};
use std::sync::Arc;

/// A tool's context for one call the server checked: who calls, what the call does, and the
/// DSPs it is about. Cloned freely; each clone is the same call.
#[derive(Clone)]
pub struct Cx {
    state: Arc<State>,
    caller: Arc<Caller>,
    tool: &'static dyn AnyTool,
    effect: Effect,
    dsps: Arc<[Dsp]>,
}
impl Cx {
    pub(super) fn new(
        state: Arc<State>,
        caller: Caller,
        tool: &'static dyn AnyTool,
        effect: Effect,
        dsps: Vec<Dsp>,
    ) -> Self {
        Self {
            state,
            caller: Arc::new(caller),
            tool,
            effect,
            dsps: dsps.into(),
        }
    }
    /// The key or app calling, as it stood when the call was checked.
    pub fn caller(&self) -> &Caller {
        &self.caller
    }
    /// What the call does, as the tool said of its arguments.
    pub fn effect(&self) -> Effect {
        self.effect
    }
    /// The DSP a call about one DSP is about: one the connection reaches, with the tool's
    /// features on.
    pub fn dsp(&self) -> &Dsp {
        match &*self.dsps {
            [dsp] if self.tool.scope() == super::Scope::Dsp => dsp,
            _ => panic!("{} is not about one DSP", self.tool.name()),
        }
    }
    /// The DSPs the call is about, each one the connection reaches with the tool's features
    /// on: the one for a tool about one DSP, those named or every one for a tool about
    /// several, and none for a tool about the connection.
    pub fn dsps(&self) -> &[Dsp] {
        &self.dsps
    }

    /// Reads, with the connection checked again first: it still stands, may still use the
    /// tool, and still reaches the call's DSPs with the tool's features on.
    pub async fn read<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store) -> Answer<T> + Send + 'static,
    ) -> Answer<T> {
        let call = self.clone();
        self.state
            .read(move |db| Ok(call.recheck(db).and_then(|_| f(db))))
            .await
            .unwrap_or_else(|error| Err(error.into()))
    }

    /// Changes something, under the database's exclusive lock, with the connection checked
    /// again first: it still stands, may still change something with the tool, and still
    /// reaches the call's DSPs with the tool's features on. Only a call the tool said changes
    /// something writes; any other's write is a failure of the tool's.
    pub async fn write<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Writing) -> Answer<T> + Send + 'static,
    ) -> Answer<T> {
        if self.effect != Effect::Changes {
            return Err(Failure::Failed(Error::new("tool_wrote_while_reading", 500)));
        }
        let call = self.clone();
        self.state
            .run(move |db| {
                Ok(call.recheck(db).and_then(|caller| {
                    f(&Writing {
                        store: db,
                        caller: &caller,
                        dsps: &call.dsps,
                    })
                }))
            })
            .await
            .unwrap_or_else(|error| Err(error.into()))
    }

    /// Whether `switch`, a feature or its part, is on at `dsp`, hidden from its members or
    /// not: for a feature the tool uses only where it is on, beyond those it needs.
    pub async fn has(&self, dsp: &Dsp, switch: &str) -> Answer<bool> {
        let (dsp, switch) = (dsp.id.clone(), switch.to_owned());
        self.read(move |db| Ok(db.features(&dsp)?.contains(&switch)))
            .await
    }

    /// The connection as it stands now, if it may still do what the call does with the tool
    /// at the call's DSPs.
    fn recheck(&self, db: &Store) -> Answer<Caller> {
        let current = db_caller(db, &self.caller)?;
        permitted(&current, self.tool, self.effect)?;
        for dsp in self.dsps.iter() {
            if !current.dsps.iter().any(|reached| reached.id == dsp.id) {
                let message = format!(
                    "This connection no longer reaches {}. Tell the user.",
                    dsp.name
                );
                return Err(Refusal::new("dsp_not_found", message).into());
            }
            if !has_all(&db.features(&dsp.id)?, self.tool.features()) {
                super::switched_on(db, self.tool, dsp)?;
            }
        }
        Ok(current)
    }
}

/// What a write is handed: the store, under the exclusive lock, and the agent's name to
/// record its changes under.
pub struct Writing<'a> {
    store: &'a Store,
    caller: &'a Caller,
    dsps: &'a [Dsp],
}
impl<'a> Writing<'a> {
    pub fn store(&self) -> &'a Store {
        self.store
    }
    /// Records a change the tool made at `dsp`, one of the call's, in its activity log, as
    /// made by the platform owner whose key or app made it, naming the agent.
    pub fn audit(
        &self,
        dsp: &str,
        action: &str,
        detail: &str,
        target: Option<&str>,
        changes: &[AuditChange],
    ) -> dispatch_core::Result<()> {
        assert!(
            self.dsps.iter().any(|reached| reached.id == dsp),
            "a change recorded at a DSP the call is not about"
        );
        let mut changes = changes.to_vec();
        changes.push(("via", None, Some(self.caller.name.clone())));
        self.store.audit_with(
            Some(&self.caller.user),
            Some(dsp),
            action,
            detail,
            target,
            &changes,
        )
    }
}
