//! Routes keeps the days Cortex's execution pages bring.
use super::stage;
use crate::backend::RoutesStore;
use dispatch_core::{
    Result, State,
    collection::{
        browser::{Collected, Pending},
        registry::AddedStorage,
    },
    db::Store,
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::routes::{Capture, JOB_KIND};
use serde_json::Value;
use std::sync::Arc;

pub struct Routes;
impl Keeper for Routes {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "routes.collect"
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    fn stage<'a>(
        &'a self,
        state: &'a Arc<State>,
        dsp: &'a str,
        job: &'a str,
        owner: &'a str,
        collected: Collected,
    ) -> Pending<'a, Collected> {
        Box::pin(async move {
            let capture: Capture = serde_json::from_value(collected.data)?;
            let staged = stage(state, dsp, job, owner, capture).await?;
            Ok(Collected {
                data: serde_json::to_value(staged)?,
                scope: collected.scope,
            })
        })
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        store.publish_routes(dsp, job, &serde_json::from_value(collected.data)?)
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        store.routes_schedule_ready(dsp)
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store.routes_jobs(dsp)
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&super::STORAGE];
        &STORAGES
    }
}
