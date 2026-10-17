//! Employee timecards: calendar navigation and explicitly scoped collections.
use super::{cards, employees::photos, sync::synced_cards};
use crate::api::types::{EmployeeTimecardResponse, Timecard};
use dispatch_core::{
    Error, Result,
    db::{Store, boolean, s},
    ensure,
    foundation::{names::display, validate as v},
};
use dispatch_paycom::{
    self as paycom,
    timecards::{EmployeeTimecardPeriod, PERIOD_DAYS},
};
use rusqlite::params;
use serde_json::{Value, json};

/// An employee's latest pay period, found through the employees' code index.
const LATEST: &str = "SELECT p.period_from,p.period_to FROM publications p \
    JOIN employees e ON e.publication_id=p.id WHERE e.code=? \
    ORDER BY p.period_to DESC,p.period_from DESC LIMIT 1";

const EARLIEST_DATE: &str = "2000-01-01";

pub(crate) fn timecard_period(
    store: &Store,
    id: &str,
    code: Option<&str>,
    date: Option<&str>,
    requested: Option<&EmployeeTimecardPeriod>,
) -> Result<EmployeeTimecardPeriod> {
    if let Some(code) = code {
        v::code(code)?;
    }
    let db = store.collector(id, paycom::PROVIDER)?;
    let latest = if let Some(code) = code {
        db.one_as::<EmployeeTimecardPeriod>(LATEST, [code])?
    } else {
        db.one_as::<EmployeeTimecardPeriod>(
            "SELECT period_from,period_to FROM publications \
             ORDER BY period_to DESC,period_from DESC LIMIT 1",
            [],
        )?
    }
    .ok_or_else(|| Error::new("timecard_period_unavailable", 404))?;
    let period = if let Some(date) = date {
        v::date(date)?;
        let day: chrono::NaiveDate = date.parse().map_err(|_| Error::new("invalid_date", 400))?;
        latest.shift((day - latest.start()?).num_days().div_euclid(PERIOD_DAYS))?
    } else {
        requested.unwrap_or(&latest).clone()
    };
    let offset = (latest.start()? - period.start()?).num_days();
    ensure(
        period.from.as_str() >= EARLIEST_DATE && offset >= 0 && offset % PERIOD_DAYS == 0,
        "invalid_period",
        400,
    )?;
    Ok(period)
}

pub(crate) fn paycom_employee(store: &Store, id: &str, code: &str) -> Result<Value> {
    v::code(code)?;
    let mut employee = store
        .collector(id, paycom::PROVIDER)?
        .one(
            "SELECT e.code,e.name,e.department,e.position,e.station,e.active \
         FROM employees e JOIN publications p ON p.id=e.publication_id \
         WHERE e.code=? ORDER BY p.period_to DESC,p.period_from DESC,\
         p.collected_at DESC,p.rowid DESC LIMIT 1",
            [code],
        )?
        .ok_or_else(|| Error::new("employee_not_found", 404))?;
    boolean(&mut employee, &["active"]);
    Ok(employee)
}

/// Unfiltered, DSP-local metadata for every known employee, including older rosters.
pub(crate) fn known_paycom_employees(store: &Store, id: &str) -> Result<Vec<Value>> {
    let mut employees = store.collector(id, paycom::PROVIDER)?.all(
        "WITH newest AS (SELECT e.code,e.name,e.department,e.position,e.station,e.active, \
         row_number() OVER (PARTITION BY e.code COLLATE NOCASE ORDER BY p.period_to DESC, \
         p.period_from DESC,p.collected_at DESC,p.rowid DESC) ordinal \
         FROM employees e JOIN publications p ON p.id=e.publication_id) \
         SELECT code,name,department,position,station,active FROM newest WHERE ordinal=1 ORDER BY code",
        [],
    )?;
    ensure(employees.len() <= 5000, "invalid_workforce", 502)?;
    for employee in &mut employees {
        boolean(employee, &["active"]);
    }
    Ok(employees)
}
pub(crate) fn employee_timecard(
    store: &Store,
    id: &str,
    code: &str,
    requested: Option<&EmployeeTimecardPeriod>,
) -> Result<EmployeeTimecardResponse> {
    let mut employee = paycom_employee(store, id, code)?;
    let db = store.collector(id, paycom::PROVIDER)?;
    let latest = db
        .one_as::<EmployeeTimecardPeriod>(LATEST, [code])?
        .ok_or_else(|| Error::new("employee_not_found", 404))?;
    let period = requested.unwrap_or(&latest);
    let offset = (latest.start()? - period.start()?).num_days();
    ensure(
        period.from.as_str() >= EARLIEST_DATE && offset >= 0 && offset % PERIOD_DAYS == 0,
        "invalid_period",
        400,
    )?;
    let publication = db.one(
        "SELECT p.id,p.collected_at FROM publications p JOIN employees e ON e.publication_id=p.id \
         WHERE e.code=? AND p.period_from=? AND p.period_to=? \
         ORDER BY p.collected_at DESC,p.rowid DESC LIMIT 1",
        params![code, period.from, period.to],
    )?;
    let mut collected_at = publication
        .as_ref()
        .map(|p| s(p, "collected_at").to_owned());
    let mut timecards = if let Some(p) = &publication {
        cards(
            &db,
            "SELECT t.employee_code employeeCode,t.date,t.hours,t.status,t.punches,u.url \
             sourceUrl FROM timecards t LEFT JOIN timecard_sources u ON \
             u.publication_id=t.publication_id AND u.employee_code=t.employee_code \
             WHERE t.publication_id=? AND t.employee_code=? ORDER BY t.date DESC",
            [s(p, "id"), code],
        )?
    } else {
        vec![]
    };
    if let Some(sync) = db.one(
        "SELECT data,collected_at FROM employee_timecard_syncs WHERE employee_code=? \
         AND period_from=? AND period_to=? AND collected_at>=?",
        params![
            code,
            period.from,
            period.to,
            collected_at.as_deref().unwrap_or("")
        ],
    )? {
        timecards = synced_cards(&serde_json::from_str::<Value>(s(&sync, "data"))?);
        timecards.sort_by(|a, b| s(b, "date").cmp(s(a, "date")));
        collected_at = Some(s(&sync, "collected_at").to_owned());
    }
    employee["name"] = json!(display(s(&employee, "name")));
    employee["photo"] = json!(photos(store, id)?.remove(code));
    let previous = period.shift(-1)?;
    let job = store.latest_job_requesting(
        id,
        paycom::timecards::JOB_KIND,
        &[
            ("employeeCode", code),
            ("from", period.from.as_str()),
            ("to", period.to.as_str()),
        ],
    )?;
    let timezone = store.find_dsp(id)?.timezone;
    Ok(EmployeeTimecardResponse {
        employee: serde_json::from_value(employee)?,
        timecards: timecards
            .into_iter()
            .map(|card| Ok(serde_json::from_value::<Timecard>(card)?.assessed(&timezone)))
            .collect::<Result<_>>()?,
        period: period.clone(),
        collected_at,
        previous_period: (previous.from.as_str() >= EARLIEST_DATE).then_some(previous),
        next_period: (period != &latest).then(|| period.shift(1)).transpose()?,
        sync_status: job.map(|job| job.status),
    })
}

#[cfg(test)]
#[path = "../../tests/backend/punches/timecards.rs"]
mod tests;
