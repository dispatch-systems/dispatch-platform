//! Cortex's meal breaks as a collection finds them: each itinerary, validated against
//! the run's scope, is shown beside the published day until the job ends.
use super::{
    PROVIDER,
    discovery::{CollectionRequest, Scope},
    meals::Capture,
};
use crate::{Error, Result, State, db::s, ensure, live_collection};
use rusqlite::params;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct Writer {
    state: Arc<State>,
    job: String,
    owner: String,
}
impl Writer {
    pub fn new(state: Arc<State>, job: &str, owner: &str) -> Self {
        Self {
            state,
            job: job.into(),
            owner: owner.into(),
        }
    }
    pub async fn start_cortex(&self, scope: &Scope, drivers: Value) -> Result<()> {
        let scope = scope.clone();
        let metadata = json!({"from":scope.date,"to":scope.date,"scope":scope,"drivers":drivers});
        let job = self.job.clone();
        let owner = self.owner.clone();
        let state = self.state.clone();
        self.state
            .run_bookkeeping(move |db| {
                let row = db.job_row(&job, None)?;
                let request: CollectionRequest = serde_json::from_str(&row.request)?;
                request.validate_scope(&scope)?;
                state
                    .read_cache
                    .invalidate_tenant(&row.dsp_id, crate::read_cache::DataDomain::Live);
                db.start_live(&job, &owner, &metadata)
            })
            .await
    }
    pub async fn cortex_drivers(&self, drivers: Value) -> Result<()> {
        let job = self.job.clone();
        let owner = self.owner.clone();
        let state = self.state.clone();
        let dsp = self
            .state
            .run_bookkeeping(move |db| {
                let dsp = db.guard(&job, &owner)?;
                state
                    .read_cache
                    .invalidate_tenant(&dsp.id, crate::read_cache::DataDomain::Live);
                db.collector(&dsp.id, PROVIDER)?.exec(
                    "UPDATE collection_live_runs \
                SET metadata=json_set(metadata,'$.drivers',json(?1)) WHERE job_id=?2 AND owner=?3",
                    params![drivers.to_string(), job, owner],
                )?;
                Ok(dsp.id)
            })
            .await?;
        self.state.updates.changed(
            &dsp,
            crate::contracts::CollectionChange::provider(PROVIDER.id()),
        );
        Ok(())
    }
    pub async fn cortex(&self, capture: &Capture) -> Result<()> {
        // One complete itinerary, reduced to the four requested meal timestamps.
        capture.validate(&capture.scope)?;
        ensure(capture.itineraries.len() == 1, "invalid_live_capture", 502)?;
        let data = serde_json::to_string(capture)?;
        ensure(data.len() <= 64 * 1024, "invalid_live_capture", 502)?;
        let change = crate::contracts::CollectionChange {
            provider: PROVIDER.id().into(),
            dates: vec![capture.scope.date.clone()],
            employee_code: None,
            roster: false,
        };
        let capture = capture.clone();
        let job = self.job.clone();
        let owner = self.owner.clone();
        let state = self.state.clone();
        let dsp = self
            .state
            .run_bookkeeping(move |db| {
                let dsp = db.guard(&job, &owner)?;
                state
                    .read_cache
                    .invalidate_tenant(&dsp.id, crate::read_cache::DataDomain::Live);
                let row = db.job_row(&job, None)?;
                ensure(
                    row.kind.as_str() == PROVIDER.job_kind(),
                    "unsupported_collector",
                    409,
                )?;
                let storage = db.collector(&dsp.id, PROVIDER)?;
                let run = storage
                    .one(
                        "SELECT metadata FROM collection_live_runs WHERE job_id=? \
                AND owner=?",
                        [&job, &owner],
                    )?
                    .ok_or_else(|| Error::new("invalid_live_capture", 502))?;
                let metadata: Value = serde_json::from_str(s(&run, "metadata"))?;
                let expected: Scope = serde_json::from_value(metadata["scope"].clone())?;
                capture.validate(&expected)?;
                live_collection::stage_item(
                    &storage,
                    &job,
                    &owner,
                    &capture.itineraries[0].id,
                    &capture.scope.date,
                    &data,
                )?;
                Ok(dsp.id)
            })
            .await?;
        self.state.updates.changed(&dsp, change);
        Ok(())
    }
}
