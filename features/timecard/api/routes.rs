//! What Paycom and Cortex collected: employees, timecards, meal breaks and the Paycom
//! preferences, and the jobs and schedules that collect them.
use crate::{
    collectors::{
        cortex::{self, discovery::Scope},
        paycom,
    },
    contracts::{EmployeeTimecardPeriod, PaycomSettings},
    workforce::TimecardStore,
};
use dispatch_core::{
    Error, Result,
    accounts::api::requests::CollectionRequest,
    collection::api::routes::{
        connections,
        jobs::{job_cancel, job_list},
        schedules::schedule_routes,
    },
    db::Store,
    foundation::validate as v,
    server::http::{
        input::{Input, Reply, descending, optional, optional_text, query_number},
        route::{Dsp, Member, Route, read, write},
    },
};
use serde_json::json;

const VIEW: Dsp = Dsp("timecard.view");
const MANAGE: Dsp = Dsp("timecard.manage");
const RUN: Dsp = Dsp("collections.run");
/// What the page keeps: the only jobs its job routes list or cancel. Every other feature's
/// jobs have routes of its own.
const KINDS: &[&str] = &[paycom::timecards::JOB_KIND, cortex::meals::JOB_KIND];
// The page's tabs. Each owns no permission, so its routes ask for the tab itself.
const DAILY: &str = "timecard.daily";
const MEAL_BREAKS: &str = "timecard.meal_breaks";
const EMPLOYEES: &str = "timecard.employees";
const SORTS: &[&str] = &[
    "name",
    "hours",
    "inDay",
    "outLunch",
    "inLunch",
    "outDay",
    "totalHours",
    "condition",
];

pub fn routes() -> Vec<Route> {
    let mut routes = vec![
        read("/api/dsp/employees", VIEW, employees).logged(),
        read("/api/dsp/employees/{code}", VIEW, employee).logged(),
        write(
            "/api/dsp/employees/{code}/sync",
            Dsp("collections.run"),
            sync_employee,
        ),
        read("/api/dsp/timecards", VIEW, timecards).logged(),
        read("/api/dsp/paycom/status", VIEW, paycom_status),
        read("/api/dsp/paycom/settings", VIEW, paycom_settings).logged(),
        write("/api/dsp/paycom/settings", MANAGE, save_paycom_settings),
        read("/api/dsp/paycom/meal-breaks", VIEW, meal_comparison).logged(),
        read("/api/dsp/cortex/meal-breaks", VIEW, cortex_meal_breaks),
        job_list("/api/dsp/jobs", RUN, KINDS).logged(),
        write("/api/dsp/jobs", RUN, collect),
        job_cancel("/api/dsp/jobs/{id}/cancel", RUN, KINDS),
        read("/api/dsp/jobs/meal-breaks", VIEW, meal_sync_status),
        write("/api/dsp/jobs/meal-breaks", RUN, sync_meal_breaks),
        write(
            "/api/dsp/cortex/meal-breaks/collect",
            RUN,
            collect_cortex_meal_breaks,
        ),
    ];
    routes.extend(schedule_routes("/api/dsp/schedules", MANAGE, "timecard"));
    routes
}

fn employees(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let q = &input.query;
    v::fields(q, &["q", "direction", "offset", "limit", "status"])?;
    let query = optional_text(q, "q", 100)?;
    let desc = descending(q)?;
    let offset = query_number(q, "offset", 0, 0, 100000)?;
    let limit = if q["limit"] == "all" {
        None
    } else {
        Some(query_number(q, "limit", 50, 1, 100)?)
    };
    let status = optional(q, "status", |q, key| {
        v::choice(q, key, &["all", "active", "inactive"])
    })?
    .unwrap_or("all");
    let active = match status {
        "active" => Some(true),
        "inactive" => Some(false),
        _ => None,
    };
    let page = db.timecard_employees(c.dsp_id(), query, offset, limit, desc, active)?;
    Reply::of(&page)
}

fn employee(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let q = &input.query;
    v::fields(q, &["from", "to"])?;
    let period = if q.get("from").is_some() || q.get("to").is_some() {
        let from = v::text(q, "from", 10, 10)?;
        let to = v::text(q, "to", 10, 10)?;
        v::date(from)?;
        v::date(to)?;
        dispatch_core::ensure(from <= to, "invalid_period", 400)?;
        Some(EmployeeTimecardPeriod {
            from: from.into(),
            to: to.into(),
        })
    } else {
        None
    };
    Reply::of(&db.employee_timecard(c.dsp_id(), input.param("code"), period.as_ref())?)
}

fn timecards(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    let q = &input.query;
    v::fields(q, &["date", "sort", "direction"])?;
    let sort = optional(q, "sort", |q, key| v::choice(q, key, SORTS))?.unwrap_or("name");
    let date = v::text(q, "date", 10, 10)?;
    Reply::of(&db.daily_timecards(c.dsp_id(), date, sort, descending(q)?)?)
}

fn sync_employee(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let body = &input.body;
    v::fields(body, &["requestId", "from", "to"])?;
    let period = EmployeeTimecardPeriod {
        from: v::text(body, "from", 10, 10)?.into(),
        to: v::text(body, "to", 10, 10)?.into(),
    };
    let code = input.param("code");
    let job = db.enqueue_employee_timecard(
        c.dsp_id(),
        Some(c.actor()),
        v::text(body, "requestId", 1, 128)?,
        code,
        &period,
    )?;
    c.audit(
        db,
        "collection.requested",
        &format!("{code} {}–{}", period.from, period.to),
    )?;
    Ok(Reply::status(job, 202))
}

fn paycom_status(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let publication = db
        .collector(c.dsp_id(), paycom::PROVIDER)?
        .one("SELECT collected_at FROM publications WHERE active=1", [])?;
    Ok(Reply::json(json!({
        "connection":connections::summary(db, c, paycom::PROVIDER)?,
        "workforce":{"collectedAt":publication.map(|p| p["collected_at"].clone())}
    })))
}

// The change history names people, so only those who manage timecards see it.
fn paycom_settings(db: &Store, c: &Member, _: &Input) -> Result<Reply> {
    let mut value: PaycomSettings = serde_json::from_value(db.timecard_preferences(c.dsp_id())?)?;
    if !c.can("timecard.manage") {
        value.history.clear();
    }
    Reply::of(&value)
}

fn save_paycom_settings(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    v::fields(b, &["revision", "values"])?;
    let revision = v::integer(b, "revision", 0, i64::MAX)?;
    let saved = db.save_timecard_preferences(c.dsp_id(), c.actor(), revision, &b["values"])?;
    Reply::of(&serde_json::from_value::<PaycomSettings>(saved)?)
}

fn meal_comparison(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    // Live pages depend on current job leases, actor permissions and connection revision.
    // Check them afresh during collection rather than caching that moving visibility.
    if db.any_active_job(c.dsp_id(), KINDS)? {
        return Reply::of(&db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str())?);
    }
    let comparison = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::meals::CACHED, c.dsp_id()),
        format!("meals:{}:{}:{}", c.dsp_id(), date, c.dsp.timezone),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str()),
    )?;
    Ok(Reply::encoded(comparison))
}

fn cortex_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(Reply::json(db.meal_publications(c.dsp_id(), date)?))
}
fn collect(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let request = CollectionRequest::parse(&input.body, false)?;
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let job = if let Some(date) = &request.date {
        db.enqueue_paycom_date(id, actor, &request.request_id, date)?
    } else {
        db.enqueue_timecards(id, actor, &request.request_id)?
    };
    let date = request.date.as_deref().unwrap_or("");
    db.audit(actor, Some(id), "collection.requested", date)?;
    Ok(Reply::status(job, 202))
}

fn meal_sync_status(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(Reply::json(db.meal_sync_status(c.dsp_id(), date)?))
}

fn sync_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let request = CollectionRequest::parse(&input.body, true)?;
    let date = request
        .date
        .as_deref()
        .ok_or_else(|| Error::new("invalid_input", 400))?;
    let (id, actor) = (c.dsp_id(), c.actor());
    let result = db.enqueue_meal_sync(id, actor, &request.request_id, date)?;
    db.audit(Some(actor), Some(id), "meal_breaks.sync_requested", date)?;
    Ok(Reply::status(result, 202))
}

fn collect_cortex_meal_breaks(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    let b = &input.body;
    let scope = Scope::request(b, c.dsp.timezone.as_str())?;
    let (id, actor) = (c.dsp_id(), Some(c.actor()));
    let job = db.enqueue_meals(id, actor, v::text(b, "requestId", 1, 128)?, &scope)?;
    db.audit(actor, Some(id), "cortex.collection.requested", "")?;
    Ok(Reply::status(job, 202))
}

/// A tab switched off answers as a route that never existed.
fn tab(c: &Member, id: &str) -> Result<()> {
    dispatch_core::ensure(c.has(id), "not_found", 404)
}
