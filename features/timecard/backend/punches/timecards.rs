//! Employee timecards: calendar navigation and explicitly scoped collections.
use crate::{
    contracts::{EmployeeTimecardPeriod, EmployeeTimecardResponse, Timecard},
    workforce::{TimecardStore, cards, sync::synced_cards},
};
use dispatch_core::{
    Error, Result,
    db::{Store, boolean, s},
    ensure,
    foundation::{names::display_name, validate as v},
};
use dispatch_paycom::{self as paycom, timecards::PERIOD_DAYS};
use rusqlite::params;
use serde_json::{Value, json};

const EARLIEST_DATE: &str = "2000-01-01";

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
pub(super) fn employee_timecard(
    store: &Store,
    id: &str,
    code: &str,
    requested: Option<&EmployeeTimecardPeriod>,
) -> Result<EmployeeTimecardResponse> {
    let mut employee = paycom_employee(store, id, code)?;
    let db = store.collector(id, paycom::PROVIDER)?;
    let latest = db
        .one_as::<EmployeeTimecardPeriod>(
            "SELECT p.period_from,p.period_to FROM publications p \
         JOIN employees e ON e.publication_id=p.id WHERE e.code=? \
         ORDER BY p.period_to DESC,p.period_from DESC LIMIT 1",
            [code],
        )?
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
    let settings = store.timecard_preference_values(id)?;
    employee["name"] = json!(display_name(
        s(&employee, "name"),
        s(&settings["values"], "name_order")
    ));
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
    Ok(EmployeeTimecardResponse {
        employee: serde_json::from_value(employee)?,
        timecards: timecards
            .into_iter()
            .map(|card| Ok(serde_json::from_value::<Timecard>(card)?.assessed()))
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
