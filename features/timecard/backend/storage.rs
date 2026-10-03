//! What Timecard reads and writes for a DSP, as one extension of the store: Paycom's
//! employees, timecards and preferences, and the meal breaks Cortex reports. Each method is
//! written beside the rest of its part of Timecard.
use crate::{
    Result,
    collectors::cortex::{discovery::Scope, meals::Capture},
    contracts::{
        DailyTimecards, EmployeeTimecardPeriod, EmployeeTimecardResponse, EmployeesResponse,
        MealComparison,
    },
    db::Store,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub trait TimecardStore {
    fn daily_timecards_source(
        &self,
        id: &str,
        date: &str,
    ) -> Result<(Option<Value>, Vec<Value>, Vec<Value>)>;
    fn daily_timecards(
        &self,
        id: &str,
        date: &str,
        sort: &str,
        desc: bool,
    ) -> Result<DailyTimecards>;
    fn daily_timecards_range(
        &self,
        id: &str,
        from: &str,
        to: &str,
        sort: &str,
        desc: bool,
        codes: Option<&[String]>,
    ) -> Result<BTreeMap<String, DailyTimecards>>;
    fn timecard_employees(
        &self,
        id: &str,
        query: &str,
        offset: usize,
        limit: Option<usize>,
        desc: bool,
        active: Option<bool>,
    ) -> Result<EmployeesResponse>;
    fn timecard_preference_values(&self, id: &str) -> Result<Value>;
    fn timecard_preferences(&self, id: &str) -> Result<Value>;
    fn save_timecard_preferences(
        &self,
        id: &str,
        actor: &str,
        revision: i64,
        values: &Value,
    ) -> Result<Value>;
    fn publish_timecards(&self, id: &str, value: &Value) -> Result<Value>;
    fn enqueue_timecards(&self, id: &str, actor: Option<&str>, key: &str) -> Result<Value>;
    fn enqueue_paycom_date(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        date: &str,
    ) -> Result<Value>;
    fn enqueue_employee_timecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        code: &str,
        period: &EmployeeTimecardPeriod,
    ) -> Result<Value>;
    fn employee_timecard(
        &self,
        id: &str,
        code: &str,
        requested: Option<&EmployeeTimecardPeriod>,
    ) -> Result<EmployeeTimecardResponse>;
    fn meal_comparison(&self, id: &str, date: &str, timezone: &str) -> Result<MealComparison>;
    fn meal_comparisons(
        &self,
        id: &str,
        from: &str,
        to: &str,
        timezone: &str,
        selected: Option<&[String]>,
    ) -> Result<BTreeMap<String, MealComparison>>;
    fn publish_meals(
        &self,
        dsp: &str,
        job: &str,
        capture: &Capture,
        expected: &Scope,
    ) -> Result<Value>;
    fn meal_publications(&self, dsp: &str, date: &str) -> Result<Value>;
    fn meal_sync_status(&self, id: &str, date: &str) -> Result<Value>;
    fn enqueue_meals(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        scope: &Scope,
    ) -> Result<Value>;
    fn enqueue_meal_sync(&self, id: &str, actor: &str, key: &str, date: &str) -> Result<Value>;
}
impl TimecardStore for Store {
    fn daily_timecards_source(
        &self,
        id: &str,
        date: &str,
    ) -> Result<(Option<Value>, Vec<Value>, Vec<Value>)> {
        super::daily::daily_timecards_source(self, id, date)
    }
    fn daily_timecards(
        &self,
        id: &str,
        date: &str,
        sort: &str,
        desc: bool,
    ) -> Result<DailyTimecards> {
        super::daily::daily_timecards(self, id, date, sort, desc)
    }
    fn daily_timecards_range(
        &self,
        id: &str,
        from: &str,
        to: &str,
        sort: &str,
        desc: bool,
        codes: Option<&[String]>,
    ) -> Result<BTreeMap<String, DailyTimecards>> {
        super::daily::daily_timecards_range(self, id, from, to, sort, desc, codes)
    }
    fn timecard_employees(
        &self,
        id: &str,
        query: &str,
        offset: usize,
        limit: Option<usize>,
        desc: bool,
        active: Option<bool>,
    ) -> Result<EmployeesResponse> {
        super::employees::timecard_employees(self, id, query, offset, limit, desc, active)
    }
    fn timecard_preference_values(&self, id: &str) -> Result<Value> {
        super::preferences::timecard_preference_values(self, id)
    }
    fn timecard_preferences(&self, id: &str) -> Result<Value> {
        super::preferences::timecard_preferences(self, id)
    }
    fn save_timecard_preferences(
        &self,
        id: &str,
        actor: &str,
        revision: i64,
        values: &Value,
    ) -> Result<Value> {
        super::preferences::save_timecard_preferences(self, id, actor, revision, values)
    }
    fn publish_timecards(&self, id: &str, value: &Value) -> Result<Value> {
        super::publication::publish_timecards(self, id, value)
    }
    fn enqueue_timecards(&self, id: &str, actor: Option<&str>, key: &str) -> Result<Value> {
        super::queue::enqueue_timecards(self, id, actor, key)
    }
    fn enqueue_paycom_date(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        date: &str,
    ) -> Result<Value> {
        super::queue::enqueue_paycom_date(self, id, actor, key, date)
    }
    fn enqueue_employee_timecard(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        code: &str,
        period: &EmployeeTimecardPeriod,
    ) -> Result<Value> {
        super::queue::enqueue_employee_timecard(self, id, actor, key, code, period)
    }
    fn employee_timecard(
        &self,
        id: &str,
        code: &str,
        requested: Option<&EmployeeTimecardPeriod>,
    ) -> Result<EmployeeTimecardResponse> {
        super::timecards::employee_timecard(self, id, code, requested)
    }
    fn meal_comparison(&self, id: &str, date: &str, timezone: &str) -> Result<MealComparison> {
        crate::meals::comparison::meal_comparison(self, id, date, timezone)
    }
    fn meal_comparisons(
        &self,
        id: &str,
        from: &str,
        to: &str,
        timezone: &str,
        selected: Option<&[String]>,
    ) -> Result<BTreeMap<String, MealComparison>> {
        crate::meals::comparison::meal_comparisons(self, id, from, to, timezone, selected)
    }
    fn publish_meals(
        &self,
        dsp: &str,
        job: &str,
        capture: &Capture,
        expected: &Scope,
    ) -> Result<Value> {
        crate::meals::publish_meals(self, dsp, job, capture, expected)
    }
    fn meal_publications(&self, dsp: &str, date: &str) -> Result<Value> {
        crate::meals::meal_publications(self, dsp, date)
    }
    fn meal_sync_status(&self, id: &str, date: &str) -> Result<Value> {
        crate::meals::sync::meal_sync_status(self, id, date)
    }
    fn enqueue_meals(
        &self,
        id: &str,
        actor: Option<&str>,
        key: &str,
        scope: &Scope,
    ) -> Result<Value> {
        crate::meals::sync::enqueue_meals(self, id, actor, key, scope)
    }
    fn enqueue_meal_sync(&self, id: &str, actor: &str, key: &str, date: &str) -> Result<Value> {
        crate::meals::sync::enqueue_meal_sync(self, id, actor, key, date)
    }
}
