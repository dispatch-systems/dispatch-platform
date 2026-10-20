//! Vans keeps Cortex's `vans` collection: the fleet, published as the current one.
use super::DOMAIN;
use super::storage::{STORAGE, bind_fleet_request, fleet_request};
use crate::VansStore;
use dispatch_core::{
    Result,
    collection::{browser::Collected, registry::AddedStorage},
    db::Store,
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::vans::JOB_KIND;
use serde_json::Value;

pub(crate) struct Fleet;
impl Keeper for Fleet {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "vans.collect"
    }
    fn domain(&self) -> DataDomain {
        DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, _: &Value) -> Result<Value> {
        bind_fleet_request(store, dsp)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        store.publish_fleet(dsp, job, &serde_json::from_value(collected.data)?)
    }
    fn last_collected(&self, store: &Store, dsp: &str) -> Result<Option<String>> {
        Ok(store
            .vans_db(dsp)?
            .query_as::<(Option<String>,)>("SELECT max(collected_at) FROM fleet_runs", [])?
            .into_iter()
            .next()
            .and_then(|(at,)| at))
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        bind_fleet_request(store, dsp).map(|_| ())
    }
    fn scheduled(&self, _: &Store, _dsp: &str, _: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![("vans".into(), fleet_request())])
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&STORAGE];
        &STORAGES
    }
}
