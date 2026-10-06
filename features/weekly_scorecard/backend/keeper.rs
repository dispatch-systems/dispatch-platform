//! Weekly Scorecard keeps the weeks Cortex's performance API answers with.
use crate::backend::WeeklyScorecardStore;
use dispatch_core::{
    Error, Result,
    collection::{browser::Collected, registry::AddedStorage},
    db::{Store, s},
    ensure,
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::{discovery::Scope, weekly_scorecard::JOB_KIND};
use serde_json::Value;

pub struct WeeklyScorecard;
impl Keeper for WeeklyScorecard {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "weekly_scorecard.collect"
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        let week = request["week"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let station = request["station"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let bound = super::bind_weekly_scorecard_request(store, dsp, week)?;
        ensure(
            bound["station"] == station,
            "weekly_scorecard_scope_mismatch",
            409,
        )?;
        Ok(bound)
    }
    /// Where the last publication of `station` read the API, with the company it named.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        let address = store.weekly_scorecard_address(dsp, s(question, "station"))?;
        Ok(serde_json::to_value(address)?)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let Collected { data, scope } = collected;
        store.publish_weekly_scorecard(
            dsp,
            job,
            &serde_json::from_value(data)?,
            &Scope::collected(scope)?,
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        super::weekly_scorecard_schedule_ready(store, dsp)
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store.weekly_scorecard_jobs(dsp)
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&super::STORAGE];
        &STORAGES
    }
}
