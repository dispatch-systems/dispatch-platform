//! What Paycom and Cortex collected: employees, timecards, meal breaks and the Paycom
//! preferences, and the jobs and schedules that collect them.
use crate::{api::settings::PaycomSettings, backend::TimecardStore};
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
        route::{Dsp, Member, Route, coalesced_read, coalesced_read_by, read, write},
    },
};
use dispatch_cortex::{self as cortex, discovery::Scope};
use dispatch_paycom::{self as paycom, timecards::EmployeeTimecardPeriod};
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
        coalesced_read_by(
            "/api/dsp/employees",
            VIEW,
            |_, _, _| Ok("roster".into()),
            employees,
        )
        .logged(),
        read("/api/dsp/employees/{code}", VIEW, employee).logged(),
        write(
            "/api/dsp/employees/{code}/sync",
            Dsp("collections.run"),
            sync_employee,
        ),
        read("/api/dsp/timecards", VIEW, timecards).logged(),
        coalesced_read_by("/api/dsp/timecards/page", VIEW, day_key, timecard_page).logged(),
        coalesced_read_by(
            "/api/dsp/timecards/punches/{code}",
            VIEW,
            day_key,
            daily_punches,
        )
        .logged(),
        read("/api/dsp/paycom/status", VIEW, paycom_status),
        read("/api/dsp/paycom/settings", VIEW, paycom_settings).logged(),
        write("/api/dsp/paycom/settings", MANAGE, save_paycom_settings),
        coalesced_read("/api/dsp/paycom/meal-breaks", VIEW, meal_comparison).logged(),
        coalesced_read_by("/api/dsp/paycom/meal-breaks/page", VIEW, day_key, meal_page).logged(),
        read("/api/dsp/paycom/meal-breaks/period", VIEW, meal_period).logged(),
        coalesced_read_by(
            "/api/dsp/paycom/meal-breaks/period-gaps",
            VIEW,
            period_key,
            meal_period_gaps,
        )
        .logged(),
        read("/api/dsp/cortex/meal-breaks", VIEW, cortex_meal_breaks),
        job_list("/api/dsp/jobs", RUN, KINDS).logged(),
        write("/api/dsp/jobs", RUN, collect),
        job_cancel("/api/dsp/jobs/{id}/cancel", RUN, KINDS),
        read("/api/dsp/jobs/meal-breaks", VIEW, meal_sync_status),
        write("/api/dsp/jobs/meal-breaks", RUN, sync_meal_breaks),
        read("/api/dsp/jobs/employees", VIEW, employee_sync_status),
        write("/api/dsp/jobs/employees", RUN, sync_employees),
        write(
            "/api/dsp/cortex/meal-breaks/collect",
            RUN,
            collect_cortex_meal_breaks,
        ),
    ];
    routes.extend(schedule_routes("/api/dsp/schedules", MANAGE, "timecard"));
    routes
}

fn day_key(_: &Store, c: &Member, input: &Input) -> Result<String> {
    let date = v::text(&input.query, "date", 10, 10)?;
    Ok(format!("{date}:{}", c.dsp.timezone))
}
fn period_key(db: &Store, c: &Member, input: &Input) -> Result<String> {
    let date = v::text(&input.query, "date", 10, 10)?;
    let period = crate::backend::meals::period::gap_period(db, c.dsp_id(), date)?;
    Ok(format!(
        "{}:{}",
        serde_json::to_string(&period)?,
        c.dsp.timezone
    ))
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
    let roster = c.state.read_cache.shared(
        dispatch_core::server::cache::Scope::tenant(
            crate::backend::punches::employees::CACHED,
            c.dsp_id(),
        ),
        "roster".into(),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || crate::backend::punches::employees::roster(db, c.dsp_id()),
    )?;
    let page = roster.page(query, offset, limit, desc, active);
    Reply::of(&page)
}

fn employee(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let q = &input.query;
    v::fields(q, &["from", "to", "date"])?;
    let date = optional(q, "date", |q, key| v::text(q, key, 10, 10))?;
    dispatch_core::ensure(
        date.is_none() || (q.get("from").is_none() && q.get("to").is_none()),
        "invalid_period",
        400,
    )?;
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
    let period = if date.is_some() {
        Some(db.timecard_period(c.dsp_id(), Some(input.param("code")), date, None)?)
    } else {
        period
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

fn page_bounds(q: &serde_json::Value) -> Result<(usize, Option<usize>)> {
    let offset = query_number(q, "offset", 0, 0, 100000)?;
    let limit = if q["limit"] == "all" {
        None
    } else {
        Some(query_number(q, "limit", 100, 1, 100)?)
    };
    Ok((offset, limit))
}
fn timecard_page(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    let q = &input.query;
    v::fields(q, &["date", "sort", "direction", "offset", "limit"])?;
    let date = v::text(q, "date", 10, 10)?;
    let sort = optional(q, "sort", |q, key| v::choice(q, key, SORTS))?.unwrap_or("name");
    let desc = descending(q)?;
    let (offset, limit) = page_bounds(q)?;
    let day = cached_daily(db, c, date, sort, desc)?;
    Reply::of(&crate::api::types::DailyTimecardPage {
        total: day.rows.len(),
        timecards: crate::api::types::DailyTimecards {
            rows: day
                .rows
                .iter()
                .skip(offset)
                .take(limit.unwrap_or(usize::MAX))
                .cloned()
                .collect(),
            available: day.available,
            collected_at: day.collected_at.clone(),
        },
    })
}
fn daily_punches(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, DAILY)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let day = cached_daily(db, c, date, "name", false)?;
    let card = day
        .rows
        .iter()
        .find(|row| row.card.employee_code == input.param("code"))
        .ok_or_else(|| Error::new("not_found", 404))?;
    Reply::of(card)
}
fn cached_daily(
    db: &Store,
    c: &Member,
    date: &str,
    sort: &str,
    desc: bool,
) -> Result<std::sync::Arc<crate::api::types::DailyTimecards>> {
    let load = || db.daily_timecards(c.dsp_id(), date, sort, desc);
    let day = if db.any_active_job(c.dsp_id(), KINDS)? {
        std::sync::Arc::new(load()?)
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(
                crate::backend::punches::daily::CACHED,
                c.dsp_id(),
            ),
            format!("{date}:{sort}:{desc}"),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            load,
        )?
    };
    Ok(day)
}

fn meal_page(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    let q = &input.query;
    v::fields(
        q,
        &[
            "date",
            "q",
            "filter",
            "sort",
            "direction",
            "offset",
            "limit",
        ],
    )?;
    let date = v::text(q, "date", 10, 10)?;
    let search = optional_text(q, "q", 100)?;
    let filter = optional(q, "filter", |q, key| {
        v::choice(q, key, &["all", "late", "different", "missing", "gaps"])
    })?
    .unwrap_or("all");
    let sort = optional(q, "sort", |q, key| {
        v::choice(
            q,
            key,
            &[
                "employee",
                "inDay",
                "outDay",
                "outLunch",
                "inLunch",
                "firstDelivery",
                "lastDelivery",
                "comparison",
            ],
        )
    })?
    .unwrap_or("employee");
    let desc = descending(q)?;
    let (offset, limit) = page_bounds(q)?;
    let load = || db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str());
    let live = db.any_active_job(c.dsp_id(), KINDS)?;
    let day = if live {
        std::sync::Arc::new(load()?)
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
            format!("meal-table:{date}:{}", c.dsp.timezone),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            load,
        )?
    };
    let preferences = db.timecard_preference_values(c.dsp_id())?;
    let query = crate::backend::meals::page::Query {
        search,
        filter,
        sort,
        desc,
        offset,
        limit,
        name_order: dispatch_core::db::s(&preferences["values"], "name_order"),
    };
    let ordering = if live {
        std::sync::Arc::new(crate::backend::meals::page::order(&day, &query))
    } else {
        c.state.read_cache.shared(
            dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
            format!(
                "meal-order:{date}:{}:{}:{sort}:{desc}",
                c.dsp.timezone, query.name_order
            ),
            c.state
                .data_revision
                .load(std::sync::atomic::Ordering::Relaxed),
            || Ok(crate::backend::meals::page::order(&day, &query)),
        )?
    };
    Reply::of(&crate::backend::meals::page::page(&day, query, &ordering))
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

fn employee_sync_status(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let query = &input.query;
    v::fields(query, &["from", "to", "requestId"])?;
    let period = EmployeeTimecardPeriod {
        from: v::text(query, "from", 10, 10)?.into(),
        to: v::text(query, "to", 10, 10)?.into(),
    };
    let request_id = optional(query, "requestId", |query, key| v::text(query, key, 1, 128))?;
    Reply::of(&db.employee_sync_status(c.dsp_id(), &period, request_id)?)
}

fn sync_employees(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, EMPLOYEES)?;
    let body = &input.body;
    v::fields(body, &["requestId", "from", "to"])?;
    let period = EmployeeTimecardPeriod {
        from: v::text(body, "from", 10, 10)?.into(),
        to: v::text(body, "to", 10, 10)?.into(),
    };
    let status = db.enqueue_employee_sync(
        c.dsp_id(),
        c.actor(),
        v::text(body, "requestId", 1, 128)?,
        &period,
    )?;
    c.audit(
        db,
        "collection.requested",
        &format!("{}–{}", period.from, period.to),
    )?;
    Ok(Reply::status(serde_json::to_value(status)?, 202))
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
        dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
        format!("meals:{}:{}:{}", c.dsp_id(), date, c.dsp.timezone),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || db.meal_comparison(c.dsp_id(), date, c.dsp.timezone.as_str()),
    )?;
    Ok(Reply::encoded(comparison))
}

fn meal_period(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    let q = &input.query;
    v::fields(q, &["source", "date", "from", "to"])?;
    let source = v::text(q, "source", 1, 200)?;
    let date = optional(q, "date", |q, key| v::text(q, key, 10, 10))?;
    dispatch_core::ensure(
        date.is_none() || (q.get("from").is_none() && q.get("to").is_none()),
        "invalid_period",
        400,
    )?;
    let requested = if q.get("from").is_some() || q.get("to").is_some() {
        Some(EmployeeTimecardPeriod {
            from: v::text(q, "from", 10, 10)?.into(),
            to: v::text(q, "to", 10, 10)?.into(),
        })
    } else {
        None
    };
    Reply::of(&db.meal_period(
        c.dsp_id(),
        source,
        date,
        requested.as_ref(),
        c.dsp.timezone.as_str(),
    )?)
}

fn meal_period_gaps(db: &Store, c: &Member, input: &Input) -> Result<Reply> {
    tab(c, MEAL_BREAKS)?;
    v::fields(&input.query, &["date"])?;
    let date = v::text(&input.query, "date", 10, 10)?;
    let timezone = c.dsp.timezone.as_str();
    // As the day's comparison: read afresh while a collection runs, and cached otherwise.
    if db.any_active_job(c.dsp_id(), KINDS)? {
        return Reply::of(&db.meal_period_gaps(c.dsp_id(), date, timezone)?);
    }
    let period = crate::backend::meals::period::gap_period(db, c.dsp_id(), date)?;
    let gaps = c.state.read_cache.json(
        dispatch_core::server::cache::Scope::tenant(crate::backend::meals::CACHED, c.dsp_id()),
        format!(
            "meal-period-gaps:{}:{}",
            serde_json::to_string(&period)?,
            timezone
        ),
        c.state
            .data_revision
            .load(std::sync::atomic::Ordering::Relaxed),
        || crate::backend::meals::period::gaps_for(db, c.dsp_id(), period, timezone),
    )?;
    Ok(Reply::encoded(gaps))
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
