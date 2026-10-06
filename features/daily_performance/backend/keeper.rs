//! Daily policy, request binding and publication belong to this feature.
use crate::DailyPerformanceStore;
use dispatch_core::{
    Error, Result,
    collection::{browser::Collected, registry::AddedStorage},
    db::{Store, s},
    ensure,
    manifest::Keeper,
    server::cache::DataDomain,
};
use dispatch_cortex::{daily_performance::JOB_KIND, discovery::Scope};
use serde_json::Value;
pub(crate) struct DailyPerformanceKeeper;
impl Keeper for DailyPerformanceKeeper {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "daily_performance.collect"
    }
    fn domain(&self) -> DataDomain {
        super::storage::DOMAIN
    }
    fn bind(&self, store: &Store, dsp: &str, request: &Value) -> Result<Value> {
        let date = request["date"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input", 400))?;
        let bound = super::storage::bind(store, dsp, date)?;
        ensure(
            request["station"] == bound["station"],
            "daily_performance_scope_mismatch",
            409,
        )?;
        Ok(bound)
    }
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        let row = store.daily_performance_db(dsp)?.one(
            "SELECT d.source_url,p.company_id FROM daily_publications p JOIN daily_datasets d ON d.publication_id=p.id \
             WHERE p.station=? ORDER BY p.collected_at DESC,d.dataset LIMIT 1", [s(question,"station")])?;
        Ok(serde_json::to_value(row.map(|row| {
            (
                s(&row, "source_url").to_owned(),
                s(&row, "company_id").to_owned(),
            )
        }))?)
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        store.publish_daily_performance(
            dsp,
            job,
            &serde_json::from_value(collected.data)?,
            &Scope::collected(collected.scope)?,
        )
    }
    fn schedule_ready(&self, store: &Store, dsp: &str) -> Result<()> {
        super::storage::bind(store, dsp, &super::storage::today(store, dsp)?.to_string())
            .map(|_| ())
    }
    fn scheduled(&self, store: &Store, dsp: &str) -> Result<Vec<(String, Value)>> {
        store.daily_performance_jobs(dsp)
    }
    fn storages(&self) -> &'static [&'static AddedStorage] {
        static STORAGES: [&AddedStorage; 1] = [&super::storage::STORAGE];
        &STORAGES
    }
}
