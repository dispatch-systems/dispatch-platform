//! Bounded, supervised upkeep. Each declared task has at most one pass in flight;
//! external services never hold the scheduler's turn at starting collections. Once the first
//! pass after the server starts has ended, every task's, it logs `upkeep.settled`: the tests'
//! servers are handed over then, their data as that pass leaves it.
use crate::{
    Error, Result, State,
    collection::registry::Provider,
    db::now,
    foundation::observability,
    manifest::{self, Feature, registry},
};
use std::{collections::HashSet, sync::Arc, time::Duration};

pub(super) struct Maintenance {
    pub tasks: tokio::task::JoinSet<usize>,
    running: HashSet<usize>,
    last: Vec<i64>,
    audit_pruned: i64,
    /// Whether the first pass has ended.
    settled: bool,
}
impl Maintenance {
    pub fn new() -> Self {
        Self {
            tasks: tokio::task::JoinSet::new(),
            running: HashSet::new(),
            last: vec![0; declared().count()],
            audit_pruned: 0,
            settled: false,
        }
    }
    pub fn start(&mut self, state: &Arc<State>) {
        if self.running.insert(0) {
            let prune = now() - self.audit_pruned >= 24 * 60 * 60 * 1000;
            if prune {
                self.audit_pruned = now();
            }
            let state = state.clone();
            self.tasks.spawn(async move {
                crate::tenancy::deletion::finish(&state).await;
                if let Err(error) = cleanup(&state, prune).await {
                    super::scheduler::failed("checkpoint_cleanup_failed", &error);
                }
                0
            });
        }
        for (index, task) in declared().enumerate() {
            let id = index + 1;
            if !self.running.insert(id) {
                continue;
            }
            let due = now() - self.last[index] >= task.every().as_millis() as i64;
            if due {
                self.last[index] = now();
            }
            let work = task.run(state.clone(), due);
            self.tasks.spawn(async move {
                work.await;
                id
            });
        }
    }
    pub fn finished(
        &mut self,
        result: Option<std::result::Result<usize, tokio::task::JoinError>>,
    ) -> Result<()> {
        match result {
            Some(Ok(id)) => {
                self.running.remove(&id);
                if self.running.is_empty() && !self.settled {
                    self.settled = true;
                    observability::event("info", "upkeep.settled", serde_json::json!({}));
                }
            }
            Some(Err(_)) => return Err(Error::new("maintenance_task_failed", 500)),
            None => {}
        }
        Ok(())
    }
}
enum Task {
    Platform(&'static manifest::Maintenance),
    Dsp(&'static Feature, &'static manifest::DspMaintenance),
}
impl Task {
    fn every(&self) -> Duration {
        match self {
            Self::Platform(task) => task.every,
            Self::Dsp(_, task) => task.every,
        }
    }
    fn run(self, state: Arc<State>, due: bool) -> manifest::Upkeep {
        match self {
            Self::Platform(task) => (task.run)(state, due),
            Self::Dsp(feature, task) => Box::pin(for_dsps(state, feature, task, due)),
        }
    }
}
fn declared() -> impl Iterator<Item = Task> {
    registry()
        .agents
        .into_iter()
        .flat_map(|a| a.maintenance)
        .chain(registry().features.iter().flat_map(|f| f.maintenance))
        .map(Task::Platform)
        .chain(
            registry()
                .dsp_maintenance()
                .map(|(feature, task)| Task::Dsp(feature, task)),
        )
}

async fn for_dsps(
    state: Arc<State>,
    feature: &'static Feature,
    task: &'static manifest::DspMaintenance,
    due: bool,
) {
    let failed = |dsp: Option<&str>, error: &Error| {
        observability::event(
            "error",
            "dsp_maintenance_failed",
            serde_json::json!({"feature":feature.name,"dspId":dsp,"error":error.code}),
        );
    };
    let dsps = match state.read(|db| db.served_ids()).await {
        Ok(dsps) => dsps,
        Err(error) => return failed(None, &error),
    };
    for dsp in dsps {
        // The previous DSP's callback may have awaited outside work. Admit this DSP afresh.
        let at = dsp.clone();
        let enabled = state
            .read(move |db| {
                Ok(db.serves_dsp(&at)?
                    && feature.belongs_to(db.feature_code(&at)?.as_deref())
                    && db.feature_enabled(&at, feature.switch.id)?)
            })
            .await;
        match enabled {
            Ok(true) => (task.run)(state.clone(), dsp, due).await,
            Ok(false) => {}
            Err(error) => failed(Some(&dsp), &error),
        }
    }
}

async fn cleanup(state: &Arc<State>, prune_audit: bool) -> Result<()> {
    const EXPIRED: [&str; 2] = [
        "DELETE FROM sessions WHERE expires_at<?",
        "DELETE FROM resets WHERE expires_at<?",
    ];
    // Accepted invitations remain for Diagnostics for 90 days.
    const INVITATIONS: &str = "DELETE FROM invitations WHERE expires_at<?1 AND \
         (used_at IS NULL OR used_at<?1-7776000000)";
    let dsps: Vec<(String,)> = state
        .run_bookkeeping(move |db| {
            if prune_audit {
                db.prune_audit()?;
            }
            db.platform.transaction(|| {
                for sql in EXPIRED
                    .iter()
                    .chain(&["DELETE FROM throttle WHERE reset_at<?"])
                {
                    db.platform.exec(sql, [now()])?;
                }
                Ok(())
            })?;
            db.platform.query_as(
                "SELECT id FROM dsps WHERE status IN ('active','suspended')",
                [],
            )
        })
        .await?;
    for (dsp,) in dsps {
        let shared = state.clone();
        state
            .run_bookkeeping(move |db| {
                let people = db.dsp(&dsp)?;
                people.transaction(|| {
                    for sql in EXPIRED {
                        people.exec(sql, [now()])?;
                    }
                    if people.exec(INVITATIONS, [now()])? > 0 {
                        shared.read_cache.invalidate_listings();
                    }
                    Ok(())
                })?;
                drop(people);
                for provider in Provider::all() {
                    provider.collector().prune(db, &dsp)?;
                }
                Ok(())
            })
            .await?;
    }
    Ok(())
}
