//! DVIC keeps the inspections of the weekly reports Cortex publishes.
use crate::{
    Error, Result, browsers::Collected, collectors::cortex::dvic::JOB_KIND, db::Store, ensure,
    manifest::Keeper,
};
use serde_json::Value;

pub struct Dvic;
impl Keeper for Dvic {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn bind(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        let station = request["station"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_dvic_request", 400))?;
        let weeks: Vec<String> = serde_json::from_value(request["weeks"].clone())
            .map_err(|_| Error::new("invalid_dvic_request", 400))?;
        let bound = store.bind_dvic_request(dsp, weeks)?;
        ensure(bound["station"] == station, "dvic_scope_mismatch", 409)?;
        Ok(bound)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        store.publish_dvic(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &scope.ok_or_else(|| Error::new("invalid_cortex_scope", 502))?,
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        store.dvic_schedule_ready(dsp)
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store.dvic_jobs(dsp)
    }
}
