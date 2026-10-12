//! What a Paycom timecard collection is asked for: a pay period, or one employee's.
use crate::fixtures;
use chrono::{Duration, NaiveDate};
use dispatch_core::{
    Error, Result,
    db::{FromRow, Row},
    ensure,
    foundation::validate as v,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const JOB_KIND: &str = "paycom.collect";
pub const PERIOD_DAYS: i64 = 14;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export_to = "collectors/paycom/api/generated/"))]
pub struct EmployeeTimecardPeriod {
    pub from: String,
    pub to: String,
}
impl FromRow for EmployeeTimecardPeriod {
    fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            from: row.get("period_from")?,
            to: row.get("period_to")?,
        })
    }
}

impl EmployeeTimecardPeriod {
    pub fn start(&self) -> Result<NaiveDate> {
        v::date(&self.from)?;
        v::date(&self.to)?;
        let start: NaiveDate = self
            .from
            .parse()
            .map_err(|_| Error::new("invalid_period", 400))?;
        ensure(
            (start + Duration::days(PERIOD_DAYS - 1)).to_string() == self.to,
            "invalid_period",
            400,
        )?;
        Ok(start)
    }
    pub fn shift(&self, periods: i64) -> Result<Self> {
        let start = self.start()? + Duration::days(periods * PERIOD_DAYS);
        Ok(Self {
            from: start.to_string(),
            to: (start + Duration::days(PERIOD_DAYS - 1)).to_string(),
        })
    }
    pub(crate) fn provider_period(&self) -> Result<Value> {
        let start = self.start()?;
        Ok(
            json!({"start":self.from,"end":self.to,"key":format!("{}_{}",self.from,self.to),
            "dates":(0..PERIOD_DAYS).map(|i|(start+Duration::days(i)).to_string()).collect::<Vec<_>>()}),
        )
    }
}

/// An explicit full-workforce capture whose period cannot move while it is queued.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeriodSync {
    pub all_employees: bool,
    pub from: String,
    pub to: String,
}
impl PeriodSync {
    pub fn parse(request: &Value) -> Result<Option<Self>> {
        if request.get("allEmployees").is_none() {
            return Ok(None);
        }
        let scope: Self = dispatch_core::foundation::wire::request(request)?;
        ensure(scope.all_employees, "invalid_period", 400)?;
        scope.period().start()?;
        Ok(Some(scope))
    }
    pub fn period(&self) -> EmployeeTimecardPeriod {
        EmployeeTimecardPeriod {
            from: self.from.clone(),
            to: self.to.clone(),
        }
    }
    pub fn date(&self, timezone: &str) -> Result<NaiveDate> {
        ensure(self.all_employees, "invalid_period", 400)?;
        self.period().start()?;
        super::collection_date(&json!({"date":self.from}), timezone)?
            .ok_or_else(|| Error::new("invalid_period", 400))
    }
    pub fn validate_capture(&self, data: &Value) -> Result<()> {
        super::validate_workforce(data)?;
        self.period().start()?;
        let employees = data["employees"].as_array().unwrap();
        // Workforce validation already requires unique employee/day pairs within these
        // fourteen bounds, so this count also proves every employee has every day.
        ensure(
            self.all_employees
                && data["from"] == self.from
                && data["to"] == self.to
                && !employees.is_empty()
                && data["timecards"].as_array().unwrap().len()
                    == employees.len() * PERIOD_DAYS as usize,
            "timecard_identity_mismatch",
            502,
        )
    }
    pub fn fixture(&self, timezone: &str) -> Result<Value> {
        let start = self.date(timezone)?;
        let zone: chrono_tz::Tz = timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        let today = chrono::Utc::now().with_timezone(&zone).date_naive();
        let mut data = fixtures::fixture_date(
            timezone,
            Some((start + Duration::days(PERIOD_DAYS - 1)).min(today)),
        )?;
        ensure(
            data["from"] == self.from && data["to"] == self.to,
            "invalid_period",
            400,
        )?;
        let cards = data["timecards"].as_array().unwrap();
        let records = data["employees"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|employee| {
                (0..PERIOD_DAYS)
                    .map(|i| {
                        let date = (start + Duration::days(i)).to_string();
                        cards
                            .iter()
                            .find(|card| {
                                card["employeeCode"] == employee["code"] && card["date"] == date
                            })
                            .cloned()
                            .unwrap_or_else(|| {
                                json!({
                                    "employeeCode":employee["code"],
                                    "date":date,
                                    "hours":0,
                                    "status":"No punches",
                                    "punches":[]
                                })
                            })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        data["timecards"] = json!(records);
        self.validate_capture(&data)?;
        Ok(data)
    }
}

/// A job with this scope can only read and publish this employee's period.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmployeeSync {
    pub employee_code: String,
    pub from: String,
    pub to: String,
}
impl EmployeeSync {
    pub fn parse(request: &Value) -> Result<Option<Self>> {
        if request.get("employeeCode").is_none() {
            return Ok(None);
        }
        let scope: Self = dispatch_core::foundation::wire::request(request)?;
        v::code(&scope.employee_code)?;
        scope.period().start()?;
        Ok(Some(scope))
    }
    pub fn period(&self) -> EmployeeTimecardPeriod {
        EmployeeTimecardPeriod {
            from: self.from.clone(),
            to: self.to.clone(),
        }
    }
    pub fn fixture(&self, timezone: &str) -> Result<Value> {
        let zone: chrono_tz::Tz = timezone
            .parse()
            .map_err(|_| Error::new("invalid_timezone", 400))?;
        let today = chrono::Utc::now().with_timezone(&zone).date_naive();
        let end = self.period().start()? + Duration::days(PERIOD_DAYS - 1);
        let mut data = fixtures::fixture_date(timezone, Some(end.min(today)))?;
        data["employees"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["code"] == self.employee_code);
        ensure(
            data["employees"].as_array().unwrap().len() == 1,
            "employee_not_found",
            404,
        )?;
        let cards = data["timecards"].as_array().unwrap();
        let start = self.period().start()?;
        let records = (0..PERIOD_DAYS)
            .map(|i| {
                let date = (start + Duration::days(i)).to_string();
                cards
                    .iter()
                    .find(|card| card["employeeCode"] == self.employee_code && card["date"] == date)
                    .cloned()
                    .unwrap_or_else(|| {
                        json!({"employeeCode":self.employee_code,"date":date,
                    "hours":0,"status":"Complete","punches":[]})
                    })
            })
            .collect::<Vec<_>>();
        data["timecards"] = json!(records);
        data["from"] = json!(self.from);
        data["to"] = json!(self.to);
        Ok(data)
    }
}
