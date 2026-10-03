//! Timecard keeps Paycom's timecards: a pay period's, or one employee's.
use crate::{
    collectors::paycom::{
        self,
        timecards::{EmployeeSync, JOB_KIND},
    },
    workforce::TimecardStore,
};
use dispatch_core::{
    Result,
    collection::browser::Collected,
    db::{Db, Store, s},
    manifest::Keeper,
    server::cache::DataDomain,
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
    fn published(&self, db: &Db) -> Result<Vec<Value>> {
        let mut statement = db.0.prepare(
            "SELECT employee_code,date,hours,status,punches \
            FROM timecards WHERE publication_id=(SELECT id FROM publications WHERE active=1)",
        )?;
        let mut cards = vec![];
        for value in statement.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })? {
            let (code, date, hours, status, punches) = value?;
            cards.push(json!({"employeeCode":code,"date":date,"hours":hours,
                "status":status,"punches":serde_json::from_str::<Value>(&punches)?}));
        }
        Ok(cards)
    }
    fn scheduled(&self, _: &Store, _: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![("paycom".into(), json!({}))])
    }
}
