//! Timecard's routes, shared parsing and tab admission.
mod collections;
mod daily;
mod employees;
mod meals;

use dispatch_core::{
    Result,
    collection::api::routes::jobs::{job_cancel, job_list},
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, query_number},
        route::{Dsp, Member, Route, coalesced_read, coalesced_read_by, read, write},
    },
};
use dispatch_cortex as cortex;
use dispatch_paycom as paycom;
const VIEW: Dsp = Dsp("timecard.view");
const RUN: Dsp = Dsp("collections.run");
/// What the page keeps: the only jobs its job routes list or cancel. Every other feature's
/// jobs have routes of its own.
const KINDS: &[&str] = &[paycom::timecards::JOB_KIND, cortex::meals::JOB_KIND];
// The page's tabs. Each owns no permission, so its routes ask for the tab itself.
const DAILY: &str = "timecard.daily";
const MEAL_BREAKS: &str = "timecard.meal_breaks";
const EMPLOYEES: &str = "timecard.employees";

pub fn routes() -> Vec<Route> {
    vec![
        coalesced_read_by(
            "/api/dsp/employees",
            VIEW,
            employees::roster_key,
            employees::employees,
        )
        .logged(),
        read("/api/dsp/employees/{code}", VIEW, employees::employee).logged(),
        // Queuing a collection writes only its job and its log entry, which no cached read
        // uses: the caches stay as they are, as for each queuing below.
        write(
            "/api/dsp/employees/{code}/sync",
            Dsp("collections.run"),
            employees::sync_employee,
        )
        .bookkeeping(),
        read("/api/dsp/timecards", VIEW, daily::timecards).logged(),
        coalesced_read_by(
            "/api/dsp/timecards/page",
            VIEW,
            daily::daily_key,
            daily::timecard_page,
        )
        .logged(),
        coalesced_read_by(
            "/api/dsp/timecards/punches/{code}",
            VIEW,
            daily::daily_key,
            daily::daily_punches,
        )
        .logged(),
        read("/api/dsp/paycom/status", VIEW, collections::paycom_status),
        read(
            "/api/dsp/paycom/settings",
            VIEW,
            collections::paycom_settings,
        )
        .logged(),
        coalesced_read("/api/dsp/paycom/meal-breaks", VIEW, meals::meal_comparison).logged(),
        coalesced_read_by(
            "/api/dsp/paycom/meal-breaks/page",
            VIEW,
            meals::meal_key,
            meals::meal_page,
        )
        .logged(),
        read(
            "/api/dsp/paycom/meal-breaks/period",
            VIEW,
            meals::meal_period,
        )
        .logged(),
        coalesced_read_by(
            "/api/dsp/paycom/meal-breaks/period-gaps",
            VIEW,
            meals::period_key,
            meals::meal_period_gaps,
        )
        .logged(),
        read(
            "/api/dsp/cortex/meal-breaks",
            VIEW,
            meals::cortex_meal_breaks,
        ),
        job_list("/api/dsp/jobs", RUN, KINDS).logged(),
        write("/api/dsp/jobs", RUN, collections::collect).bookkeeping(),
        job_cancel("/api/dsp/jobs/{id}/cancel", RUN, KINDS),
        read(
            "/api/dsp/jobs/meal-breaks",
            VIEW,
            collections::meal_sync_status,
        ),
        write(
            "/api/dsp/jobs/meal-breaks",
            RUN,
            collections::sync_meal_breaks,
        )
        .bookkeeping(),
        read(
            "/api/dsp/jobs/employees",
            VIEW,
            employees::employee_sync_status,
        ),
        write("/api/dsp/jobs/employees", RUN, employees::sync_employees).bookkeeping(),
        write(
            "/api/dsp/cortex/meal-breaks/collect",
            RUN,
            collections::collect_cortex_meal_breaks,
        )
        .bookkeeping(),
    ]
}

fn day_key(_: &Store, c: &Member, input: &Input) -> Result<String> {
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(format!("{date}:{}", c.dsp.timezone))
}

fn page_bounds(q: &serde_json::Value) -> Result<(usize, Option<usize>)> {
    let offset = query_number(q, "offset", 0, 0, 100000)?;
    let limit = if q["limit"] == "all" {
        None
    } else {
        Some(query_number(q, "limit", 100, 1, 100)?)
    };
    Ok((offset, limit))
}

/// A tab switched off answers as a route that never existed.
fn tab(c: &Member, id: &str) -> Result<()> {
    dispatch_core::ensure(c.has(id), "not_found", 404)
}
