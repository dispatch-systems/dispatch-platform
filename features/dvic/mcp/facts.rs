//! DVIC's facts for agents: each driver's inspections in a period, the days its reports
//! cover, and how they join a driver's days and the team's table.
use super::DVIC;
use crate::backend::DvicStore;
use dispatch_core::{
    Result,
    accounts::api::types::Dsp,
    db::{Store, n, s},
    mcp::{
        api::types::{AgentArea, DriverSource},
        data::{
            facts::{Coverage, Daily, Facts},
            scope::{People, Period, Person},
        },
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// One vehicle inspection.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub date: String,
    pub transporter_id: String,
    pub driver_name: String,
    pub vehicle_type: String,
    pub inspection_type: String,
    pub started: String,
    pub seconds: i64,
    pub minimum_seconds: i64,
    pub short: bool,
}
/// Inspections in a period, every driver's or only `drivers`' transporter IDs.
pub fn inspections(
    db: &Store,
    dsp: &Dsp,
    period: &Period,
    drivers: Option<&[String]>,
) -> Result<(Vec<Inspection>, Coverage)> {
    let scope = InspectionQuery::new(db, dsp, period, drivers)?;
    Ok((scope.list(0, None)?, scope.coverage))
}

/// A short-inspection scope shared by counts, summaries and bounded detail reads.
pub struct InspectionQuery<'a> {
    data: dispatch_core::db::DspLease<'a>,
    sql: String,
    params: Vec<String>,
    pub coverage: Coverage,
}
impl<'a> InspectionQuery<'a> {
    pub fn new(
        db: &'a Store,
        dsp: &Dsp,
        period: &Period,
        drivers: Option<&[String]>,
    ) -> Result<Self> {
        let station = db.profile(&dsp.id)?.station_code;
        let data = db.dvic_db(&dsp.id)?;
        let mut held = BTreeSet::new();
        for report in data.all(
            "SELECT min_date,max_date FROM dvic_reports WHERE station=? AND scope_verified=1 AND min_date IS NOT NULL",
            [&station],
        )? {
            held.extend(period.days().into_iter().filter(|day| {
                day.as_str() >= s(&report, "min_date") && day.as_str() <= s(&report, "max_date")
            }));
        }
        let mut sql = String::from(
            " FROM dvic_inspections WHERE station=? AND scope_verified=1 AND short=1 AND start_date BETWEEN ? AND ?",
        );
        let mut params = vec![station, period.first(), period.last()];
        if let Some(ids) = drivers {
            if ids.is_empty() {
                sql.push_str(" AND 0");
            } else {
                sql.push_str(&format!(
                    " AND transporter_id IN ({})",
                    vec!["?"; ids.len()].join(",")
                ));
                params.extend_from_slice(ids);
            }
        }
        Ok(Self {
            data,
            sql,
            params,
            coverage: Coverage::of(true, held, period),
        })
    }
    pub fn count(&self) -> Result<usize> {
        Ok(self.data.count(
            &format!("SELECT COUNT(*){}", self.sql),
            rusqlite::params_from_iter(&self.params),
        )? as usize)
    }
    pub fn groups(&self) -> Result<Vec<Value>> {
        self.data.all(&format!(
            "SELECT transporter_id,transporter_name,COUNT(*) inspections,MIN(CAST(ROUND(duration_seconds) AS INTEGER)) \
             shortest{} GROUP BY transporter_id,transporter_name", self.sql
        ), rusqlite::params_from_iter(&self.params))
    }
    pub fn list(&self, offset: usize, limit: Option<usize>) -> Result<Vec<Inspection>> {
        let mut sql = format!(
            "SELECT start_date,transporter_id,transporter_name,fleet_type,inspection_type,start_time,duration_seconds,\
             minimum_seconds,short{} ORDER BY start_date,start_time,company_id,inspection_key",
            self.sql
        );
        let mut params = self.params.clone();
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ? OFFSET ?");
            params.extend([limit.to_string(), offset.min(i64::MAX as usize).to_string()]);
        }
        Ok(self
            .data
            .all(&sql, rusqlite::params_from_iter(&params))?
            .iter()
            .map(|r| Inspection {
                date: s(r, "start_date").into(),
                transporter_id: s(r, "transporter_id").into(),
                driver_name: s(r, "transporter_name").into(),
                vehicle_type: s(r, "fleet_type").into(),
                inspection_type: s(r, "inspection_type").into(),
                started: s(r, "start_time").into(),
                seconds: r["duration_seconds"].as_f64().unwrap_or(0.0).round() as i64,
                minimum_seconds: n(r, "minimum_seconds"),
                short: true,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../tests/backend/mcp/facts.rs"]
mod tests;

/// The latest day DVIC's reports cover.
pub fn fresh(db: &Store, dsp: &Dsp, station: &str) -> Result<Option<Value>> {
    let dvic = db.dvic_db(&dsp.id)?.one(
        "SELECT max(max_date) day,max(checked_at) checked_at FROM dvic_reports WHERE station=? AND scope_verified=1",
        [station],
    )?;
    Ok(dvic.filter(|r| !r["day"].is_null()).map(|r| {
        serde_json::json!({
            "latestDay": r["day"], "checkedAt": r["checked_at"]
        })
    }))
}

/// Vehicle inspections, for a driver's days and the team's table.
pub struct InspectionDays;
struct Inspected(Vec<Inspection>, Coverage);
impl Daily for InspectionDays {
    fn area(&self) -> AgentArea {
        DVIC
    }
    fn coverage(&self) -> &'static str {
        "dvic"
    }
    fn records(&self) -> &'static str {
        "inspections"
    }
    fn columns(&self) -> &'static [&'static str] {
        &["inspections", "short"]
    }
    fn gather(
        &self,
        db: &Store,
        dsp: &Dsp,
        period: &Period,
        _: &People,
        person: Option<&Person>,
    ) -> Result<Box<dyn Facts>> {
        let (rows, coverage) = inspections(db, dsp, period, person.map(|p| p.amazon.as_slice()))?;
        Ok(Box::new(Inspected(rows, coverage)))
    }
    fn none(&self) -> Box<dyn Facts> {
        Box::new(Inspected(vec![], Coverage::default()))
    }
}
impl Facts for Inspected {
    fn coverage(&self) -> &Coverage {
        &self.1
    }
    fn count(&self) -> usize {
        self.0.len()
    }
    fn whose(&self, record: usize) -> (&str, DriverSource, &str, &str) {
        let inspection = &self.0[record];
        (
            &inspection.date,
            DriverSource::Amazon,
            &inspection.transporter_id,
            &inspection.driver_name,
        )
    }
    fn record(&self, record: usize) -> Value {
        json!(self.0[record])
    }
    fn line(&self, date: &str, records: &[usize]) -> Vec<Value> {
        if !self.1.days.iter().any(|day| day == date) {
            return vec![Value::Null, Value::Null];
        }
        let short = records.iter().filter(|&&r| self.0[r].short).count();
        vec![json!(records.len()), json!(short)]
    }
    fn totals(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("inspections", json!(self.0.len())),
            (
                "short_inspections",
                json!(self.0.iter().filter(|i| i.short).count()),
            ),
        ]
    }
    fn metric(&self, name: &str, records: &[usize]) -> Option<Value> {
        match name {
            "inspections" => Some(json!(records.len())),
            "short_inspections" => {
                Some(json!(records.iter().filter(|&&r| self.0[r].short).count()))
            }
            _ => None,
        }
    }
}
