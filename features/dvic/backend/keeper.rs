//! DVIC keeps the inspections of the weekly reports Cortex publishes.
use crate::{
    Error, Result,
    browsers::Collected,
    collectors::{
        AddedStorage,
        cortex::{discovery::Scope, dvic::JOB_KIND},
    },
    db::{Store, s},
    ensure,
    manifest::Keeper,
    read_cache::DataDomain,
};
use serde_json::Value;

use super::{DvicStore, storage};

pub struct Dvic;
impl Keeper for Dvic {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "dvic.collect"
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        let station = request["station"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_dvic_request", 400))?;
        let weeks: Vec<String> = serde_json::from_value(request["weeks"].clone())
            .map_err(|_| Error::new("invalid_dvic_request", 400))?;
        let bound = storage::bind_dvic_request(store, dsp, weeks)?;
        ensure(bound["station"] == station, "dvic_scope_mismatch", 409)?;
        Ok(bound)
    }
    /// The reports already held for `station` and `company`, by their source.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        let known =
            storage::dvic_known(store, dsp, s(question, "station"), s(question, "company"))?;
        Ok(serde_json::to_value(known)?)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        store.publish_dvic(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &Scope::collected(scope)?,
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        storage::dvic_schedule_ready(store, dsp)
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store.dvic_jobs(dsp)
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&super::STORAGE];
        &STORAGES
    }
}
