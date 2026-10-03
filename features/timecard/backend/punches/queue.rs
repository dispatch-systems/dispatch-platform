//! The Paycom collections the Timecard page queues: the roster, one day of it, or one
//! employee's period.
use crate::{
    Result,
    collectors::paycom::{self, timecards::EmployeeSync, validation::collection_date},
    contracts::EmployeeTimecardPeriod,
    db::Store,
};
use serde_json::{Value, json};

impl Store {
    /// Queue helpers return the public JSON response used by collection requests.
    pub fn enqueue(&self, id: &str, actor: Option<&str>, key: &str) -> Result<Value> {
        self.enqueue_for(id, actor, key, paycom::PROVIDER, &json!({}))
    }
    pub fn enqueue_paycom_date(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        date: &str,
    ) -> Result<Value> {
        let request = json!({"date":date});
        collection_date(&request, &self.find_dsp(id)?.timezone)?;
        self.enqueue_for(id, actor, key, paycom::PROVIDER, &request)
    }
    pub fn enqueue_employee_timecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        code: &str,
        period: &EmployeeTimecardPeriod,
    ) -> Result<Value> {
        self.employee_timecard(id, code, Some(period))?;
        collection_date(&json!({"date":period.from}), &self.find_dsp(id)?.timezone)?;
        let scope = EmployeeSync {
            employee_code: code.into(),
            from: period.from.clone(),
            to: period.to.clone(),
        };
        self.enqueue_for(
            id,
            actor,
            key,
            paycom::PROVIDER,
            &serde_json::to_value(scope)?,
        )
    }
}
