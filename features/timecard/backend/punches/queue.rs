//! The Paycom collections the Timecard page queues: the roster, one day of it, or one
//! employee's period.
use crate::backend::TimecardStore;
use dispatch_core::{Result, db::Store};
use dispatch_paycom::{
    self as paycom,
    timecards::{EmployeeSync, EmployeeTimecardPeriod, collection_date},
};
use serde_json::{Value, json};

/// Queue helpers return the public JSON response used by collection requests.
pub(crate) fn enqueue_timecards(
    store: &Store,
    id: &str,
    actor: Option<&str>,
    key: &str,
) -> Result<Value> {
    store.enqueue_for(id, actor, key, paycom::PROVIDER, &json!({}))
}
pub(crate) fn enqueue_paycom_date(
    store: &Store,
    id: &str,
    actor: Option<&str>,
    key: &str,
    date: &str,
) -> Result<Value> {
    let request = json!({"date":date});
    collection_date(&request, &store.find_dsp(id)?.timezone)?;
    store.enqueue_for(id, actor, key, paycom::PROVIDER, &request)
}
pub(crate) fn enqueue_employee_timecard(
    store: &Store,
    id: &str,
    actor: Option<&str>,
    key: &str,
    code: &str,
    period: &EmployeeTimecardPeriod,
) -> Result<Value> {
    store.employee_timecard(id, code, Some(period))?;
    collection_date(&json!({"date":period.from}), &store.find_dsp(id)?.timezone)?;
    let scope = EmployeeSync {
        employee_code: code.into(),
        from: period.from.clone(),
        to: period.to.clone(),
    };
    store.enqueue_for(
        id,
        actor,
        key,
        paycom::PROVIDER,
        &serde_json::to_value(scope)?,
    )
}
