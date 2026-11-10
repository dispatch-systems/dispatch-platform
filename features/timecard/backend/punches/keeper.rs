//! Timecard keeps Paycom's timecards: a pay period's, or one employee's.
use super::payloads::{COLUMNS, punches_join};
use crate::backend::TimecardStore;
use dispatch_core::{
    Result,
    collection::browser::Collected,
    db::{Db, Store, s},
    manifest::{Keeper, Schedules, schedules},
    server::cache::DataDomain,
};
use dispatch_paycom::{
    self as paycom,
    timecards::{EmployeeSync, JOB_KIND, PeriodSync},
};
use serde_json::{Value, json};

pub struct Timecards;
impl Keeper for Timecards {
    fn keeps(&self) -> &'static str {
        JOB_KIND
    }
    fn queue_capacity(&self) -> i64 {
        15
    }
    fn permission(&self) -> &'static str {
        "collections.run"
    }
    fn schedules(&self) -> Schedules {
        schedules("timecard.manage")
    }
    fn domain(&self) -> DataDomain {
        super::DOMAIN
    }
    /// The employee a single-employee sync reads, as the latest publication holds them;
    /// everyone known, for a collection of them all; or the newest pay period collected, which
    /// a collection that isn't told its period counts from.
    fn kept(&self, store: &Store, dsp: &str, question: &Value) -> Result<Value> {
        if question["latestPeriod"] == true {
            return super::timecards::latest_period(store, dsp);
        }
        if question["allEmployees"] == true {
            return Ok(json!(super::timecards::known_paycom_employees(store, dsp)?));
        }
        super::timecards::paycom_employee(store, dsp, s(question, "employeeCode"))
    }
    fn publish(&self, store: &Store, dsp: &str, job: &str, collected: Collected) -> Result<()> {
        let request: Value = serde_json::from_str(&store.job_row(job, Some(dsp))?.request)?;
        if let Some(scope) = PeriodSync::parse(&request)? {
            scope.date(&store.find_dsp(dsp)?.timezone)?;
            scope.validate_capture(&collected.data)?;
            let collected_codes = collected.data["employees"]
                .as_array()
                .unwrap()
                .iter()
                .map(|employee| s(employee, "code").to_ascii_uppercase())
                .collect::<std::collections::BTreeSet<_>>();
            dispatch_core::ensure(
                super::timecards::known_paycom_employees(store, dsp)?
                    .iter()
                    .all(|employee| {
                        collected_codes.contains(&s(employee, "code").to_ascii_uppercase())
                    }),
                "roster_not_complete",
                502,
            )?;
            store.publish_timecards(dsp, &collected.data)?;
        } else if let Some(scope) = EmployeeSync::parse(&request)? {
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
        super::daily::cards(
            db,
            &format!(
                "SELECT t.employee_code employeeCode,t.date,t.hours,t.status,t.punches,{COLUMNS} \
                 FROM timecards t {} \
                 WHERE t.publication_id=(SELECT id FROM publications WHERE active=1)",
                punches_join()
            ),
            [],
        )
    }
    fn scheduled(&self, _: &Store, _: &str, _: &str) -> Result<Vec<(String, Value)>> {
        Ok(vec![("paycom".into(), json!({}))])
    }
}
