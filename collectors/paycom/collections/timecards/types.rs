//! What a Paycom timecard collection is asked for: a pay period, or one employee's.
use super::fixtures;
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
pub(crate) const PERIOD_DAYS: i64 = 14;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
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
    pub(crate) fn start(&self) -> Result<NaiveDate> {
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
    pub(crate) fn shift(&self, periods: i64) -> Result<Self> {
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

/// A job with this scope can only read and publish this employee's period.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EmployeeSync {
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
