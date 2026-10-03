//! Timecard keeps Paycom's timecards: a pay period's, or one employee's.
use crate::{
    Result,
    browsers::Collected,
    collectors::paycom::{
        self,
        timecards::{EmployeeSync, JOB_KIND},
    },
    db::{Store, s},
    manifest::Keeper,
    read_cache::DataDomain,
    workforce::TimecardStore,
};
use serde_json::{Value, json};

pub struct Timecards;
impl Keeper for Timecards {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn permission(&self) -> &'static str {
        "collections.run"
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    /// The employee a single-employee sync reads, as the latest publication holds them.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        super::timecards::paycom_employee(store, dsp, s(question, "employeeCode"))
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let request: Value = serde_json::from_str(&store.job_row(job, Some(dsp))?.request)?;
        if let Some(scope) = EmployeeSync::parse(&request)? {
            super::sync::publish_employee_timecard(store, dsp, &scope, &collected.data)?;
        } else {
            store.publish_timecards(dsp, &collected.data)?;
        }
        Ok(())
    }
    fn collected_at(&self, store: &Store, dsp: &str, date: &str) -> Result<Option<Value>> {
        store.collector(dsp, paycom::PROVIDER)?.one(
            "SELECT collected_at FROM publications WHERE period_from<=? AND period_to>=? \
            ORDER BY collected_at DESC,id DESC LIMIT 1",
            [date, date],
        )
    }
    fn last_collected(&self, store: &Store, dsp: &str) -> Result<Option<String>> {
        let collected: Option<(String,)> = store
            .collector(dsp, paycom::PROVIDER)?
            .one_as("SELECT collected_at FROM publications WHERE active=1", [])?;
        Ok(collected.map(|(at,)| at))
    }
    fn scheduled(&self, _: &Store, _: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![("paycom".into(), json!({}))])
    }
}
