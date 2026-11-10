//! A timecard collection read through Paycom's API: the roster from its employee directory,
//! each employee's position from their record, and their cards from their punch history. It
//! brings what a collection read from Paycom's pages brings, in the same shape.
use super::{
    EmployeeSync, EmployeeTimecardPeriod, JOB_KIND, PERIOD_DAYS, PeriodSync, collect::source_url,
    collection_date, punches, roster::with_known_employees, validate_workforce,
};
use crate::{
    codes,
    rest::{Client, unreadable},
};
use chrono::{Duration, NaiveDate, TimeZone};
use chrono_tz::Tz;
use dispatch_core::{
    Error, Result,
    collection::browser::Run,
    db::{self, s},
    ensure,
    foundation::{clock, validate as v},
    manifest::registry,
};
use serde_json::{Value, json};

/// How many employees are read at once. The client paces the requests themselves.
const LANES: usize = 4;
/// The labor allocation category that holds an employee's delivery station, by its name.
const STATION: &str = "Delivery Station Code";

/// Where a collection reads from, as connecting found it.
pub(crate) struct Source<'a> {
    pub client: &'a Client,
    /// Which labor allocation category holds the delivery station.
    pub station: usize,
    /// The first day of one of the DSP's pay periods.
    pub anchor: NaiveDate,
    /// Paycom's site, where a timecard's link opens.
    pub site: &'a str,
}

/// Whether an employee code is one a request's path may carry.
pub(crate) fn path_code(code: &str) -> bool {
    (1..=32).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The address of an employee's punches between two times, in seconds since the epoch.
pub(crate) fn punch_history(code: &str, from: i64, to: i64) -> String {
    format!("api/v1/employee/{code}/punchhistory?startdate={from}&enddate={to}")
}

/// Which of the client's labor allocation categories is the delivery station's.
pub(crate) fn station_category(rows: &[Value]) -> Option<usize> {
    rows.iter()
        .find(|row| s(row, "catdesc").trim() == STATION)
        .and_then(|row| {
            row["catcode"]
                .as_u64()
                .or_else(|| s(row, "catcode").trim().parse().ok())
        })
        .map(|category| category as usize)
        .filter(|category| (1..=20).contains(category))
}

/// The pay period `date` falls in, counted in fortnights from `anchor`.
pub(crate) fn period_of(anchor: NaiveDate, date: NaiveDate) -> EmployeeTimecardPeriod {
    let start =
        anchor + Duration::days((date - anchor).num_days().div_euclid(PERIOD_DAYS) * PERIOD_DAYS);
    EmployeeTimecardPeriod {
        from: start.to_string(),
        to: (start + Duration::days(PERIOD_DAYS - 1)).to_string(),
    }
}

/// One employee as the roster keeps them, from their row of the directory. Their position
/// comes from their own record.
pub(crate) fn employee(row: &Value, station: usize) -> Result<Value> {
    let code = s(row, "eecode");
    ensure(path_code(code), codes::API_UNREADABLE.as_str(), 502)?;
    let name = match s(row, "eename").trim() {
        "" => format!(
            "{} {}",
            s(row, "firstname").trim(),
            s(row, "lastname").trim()
        ),
        name => name.to_owned(),
    };
    ensure(
        !name.trim().is_empty() && row["deptdesc"].is_string(),
        codes::API_UNREADABLE.as_str(),
        502,
    )?;
    Ok(json!({
        "code": code,
        "name": name.trim(),
        "department": row["deptdesc"],
        "position": "",
        "station": s(row, &format!("cat{station}")),
        "active": true,
    }))
}

/// An employee's position, from their record.
pub(crate) fn position(rows: &[Value]) -> Result<&str> {
    let record = rows.first().ok_or_else(unreadable)?;
    Ok(match s(record, "position").trim() {
        "" => s(record, "position_title").trim(),
        position => position,
    })
}

/// The start of `date` where the DSP is, in seconds since the epoch.
fn midnight(zone: Tz, date: NaiveDate) -> Result<i64> {
    zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
        .earliest()
        .map(|at| at.timestamp())
        .ok_or_else(|| Error::new("invalid_period", 400))
}

/// An employee's cards for `period`. Their punches are asked for from half a day either
/// side of it, so a shift that crosses its first or last midnight is read whole.
async fn cards(
    source: &Source<'_>,
    zone: Tz,
    code: &str,
    period: &EmployeeTimecardPeriod,
) -> Result<Vec<Value>> {
    ensure(path_code(code), "invalid_input", 400)?;
    let start = period.start()?;
    let from = midnight(zone, start)? - 12 * 3600;
    let to = midnight(zone, start + Duration::days(PERIOD_DAYS))? + 12 * 3600;
    let rows = source.client.all(&punch_history(code, from, to)).await?;
    punches::cards(&rows, code, period, zone)
}

/// What the feature that keeps timecards already holds for the job's DSP.
async fn kept(run: &Run<'_>, question: Value) -> Result<Value> {
    let (job, owner) = (run.job.to_owned(), run.owner.to_owned());
    run.state
        .read(move |store| {
            let dsp = store.guard(&job, &owner)?;
            registry().keeper(JOB_KIND).kept(store, &dsp.id, &question)
        })
        .await
}

fn workforce(
    source: &Source<'_>,
    employees: Vec<Value>,
    timecards: Vec<Value>,
    period: &EmployeeTimecardPeriod,
) -> Result<Value> {
    let key = period.provider_period()?;
    let sources = employees
        .iter()
        .map(|employee| {
            json!({"employeeCode":employee["code"],"periodKey":key["key"],
                "url":source_url(source.site, employee, &key)})
        })
        .collect::<Vec<_>>();
    let data = json!({"employees":employees,"timecards":timecards,"sources":sources,
        "from":period.from,"to":period.to,"collectedAt":db::iso()});
    validate_workforce(&data)?;
    Ok(data)
}

/// Collects what the job asks for: one employee's period, or everyone's.
pub(crate) async fn collect(source: &Source<'_>, run: &Run<'_>) -> Result<Value> {
    run.metrics.method("api");
    let zone: Tz = run
        .timezone
        .parse()
        .map_err(|_| Error::new("invalid_timezone", 400))?;
    if let Some(scope) = EmployeeSync::parse(run.request)? {
        // Timecard keeps the employee the sync reads.
        let employee = kept(run, json!({"employeeCode":scope.employee_code})).await?;
        let code = s(&employee, "code").to_owned();
        v::code(&code)?;
        run.progress(20, format!("Reading timecard for {code}"))
            .await?;
        let period = scope.period();
        let timecards = cards(source, zone, &code, &period).await?;
        return workforce(source, vec![employee], timecards, &period);
    }
    let scope = PeriodSync::parse(run.request)?;
    let period = match &scope {
        Some(scope) => {
            scope.date(run.timezone)?;
            scope.period()
        }
        None => period_of(
            source.anchor,
            collection_date(run.request, run.timezone)?.unwrap_or_else(|| clock::today(zone)),
        ),
    };
    run.progress(10, "Reading employee roster".into()).await?;
    let listed = source
        .client
        .all("api/v1/employeedirectory?eestatus=A")
        .await?;
    ensure(
        !listed.is_empty() && listed.len() <= 5000,
        "roster_not_complete",
        409,
    )?;
    let mut employees = listed
        .iter()
        .map(|row| employee(row, source.station))
        .collect::<Result<Vec<_>>>()?;
    let active = employees.len();
    if scope.is_some() {
        // Known people the active directory leaves out still get their period read.
        let known = kept(run, json!({"allEmployees":true})).await?;
        let known = known
            .as_array()
            .ok_or_else(|| Error::new("invalid_workforce", 502))?;
        employees = with_known_employees(employees, known, &period.provider_period()?)?;
    }
    let mut timecards = Vec::with_capacity(employees.len() * PERIOD_DAYS as usize);
    let total = employees.len();
    for (lane, group) in employees.chunks_mut(LANES).enumerate() {
        let first = lane * LANES;
        let read = group.iter().enumerate().map(|(index, employee)| {
            let code = s(employee, "code").to_owned();
            let period = &period;
            async move {
                // An employee the directory lists has a record; one known from before keeps
                // the position they were last collected with.
                let position = if first + index < active {
                    let record = source
                        .client
                        .get(&format!("api/v1/employee/{code}"))
                        .await?
                        .rows()?;
                    Some(position(&record)?.to_owned())
                } else {
                    None
                };
                Ok::<_, Error>((position, cards(source, zone, &code, period).await?))
            }
        });
        let results = futures_util::future::join_all(read).await;
        for (employee, result) in group.iter_mut().zip(results) {
            let (position, cards) = result?;
            if let Some(position) = position {
                employee["position"] = json!(position);
            }
            timecards.extend(cards);
        }
        let done = (first + group.len()).min(total);
        run.progress(
            10 + (80 * done / total) as i64,
            format!("Reading timecards ({done} of {total})"),
        )
        .await?;
    }
    let data = workforce(source, employees, timecards, &period)?;
    if let Some(scope) = scope {
        scope.validate_capture(&data)?;
    }
    Ok(data)
}

#[cfg(test)]
#[path = "../../tests/backend/collections/timecards/api.rs"]
mod tests;
