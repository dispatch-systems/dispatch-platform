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
    let station = db.profile(&dsp.id)?.station_code;
    let data = db.dvic_db(&dsp.id)?;
    // A report covers every day from its first to its last row.
    let mut held = BTreeSet::new();
    for report in data.all(
        "SELECT min_date,max_date FROM dvic_reports WHERE station=? AND scope_verified=1 AND min_date IS NOT NULL",
        [&station],
    )? {
        held.extend(period.days().into_iter().filter(|day| {
            day.as_str() >= s(&report, "min_date") && day.as_str() <= s(&report, "max_date")
        }));
    }
    let rows = data.all(
        "SELECT start_date,transporter_id,transporter_name,fleet_type,inspection_type,start_time,\
         duration_seconds,minimum_seconds,short FROM dvic_inspections \
         WHERE station=? AND scope_verified=1 AND start_date BETWEEN ? AND ? ORDER BY start_date,start_time",
        [&station, &period.first(), &period.last()],
    )?;
    let found = rows
        .iter()
        .filter(|r| drivers.is_none_or(|ids| ids.iter().any(|id| id == s(r, "transporter_id"))))
        .map(|r| Inspection {
            date: s(r, "start_date").into(),
            transporter_id: s(r, "transporter_id").into(),
            driver_name: s(r, "transporter_name").into(),
            vehicle_type: s(r, "fleet_type").into(),
            inspection_type: s(r, "inspection_type").into(),
            started: s(r, "start_time").into(),
            seconds: r["duration_seconds"].as_f64().unwrap_or(0.0).round() as i64,
            minimum_seconds: n(r, "minimum_seconds"),
            short: n(r, "short") == 1,
        })
        .collect();
    Ok((found, Coverage::of(true, held, period)))
}

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
    fn line(&self, records: &[usize]) -> Vec<Value> {
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
